use std::path::Path;
use image::GenericImageView;
use tract_onnx::prelude::*;
use tract_onnx::prelude::tract_ndarray::ArrayViewD;

// YOLOv8n 输入尺寸(ultralytics 默认导出 640x640)
const INPUT_SIZE: u32 = 640;
// 每个候选框前 4 个值是坐标(cx,cy,w,h)
const BOX_VALUES: usize = 4;
// person 置信度行: 纯检测[4框+80类, person=class0]与pose[4框+1人+17关键点]都在第 4 行
const PERSON_SCORE_INDEX: usize = BOX_VALUES;
// pose 输出 56 通道(4框+1人+17点x3); 纯检测 84 通道
const POSE_CHANNELS: usize = 56;
// COCO 17 关键点索引: 5/6=左右肩, 11/12=左右髋, 13/14=左右膝, 15/16=左右踝
const KP_SHOULDER_L: usize = 5;
const KP_SHOULDER_R: usize = 6;
const KP_HIP_L: usize = 11;
const KP_HIP_R: usize = 12;
const KP_KNEE_L: usize = 13;
const KP_KNEE_R: usize = 14;
const KP_ANKLE_L: usize = 15;
const KP_ANKLE_R: usize = 16;
// 关键点置信度低于此值不参与姿态判断
const KP_MIN_CONF: f32 = 0.3;
// 躺卧判定第一步: 肩髋中点连线的横向分量明显大于纵向(躯干接近水平, <=~34°).
// 实测坐地前倾从高处俯拍时躯干投影可达35°(横纵33/23), 真躺卧应<~25°, 取1.5
const TORSO_RATIO: f32 = 1.5;
// 第二步: 髋膝连线接近水平(与水平夹角<~30°). 弯腰/俯身/弓步劳作时躯干同样接近水平,
// 但腿呈竖直或斜向(实测弓步弯腰的腿与水平夹角36°~68°), 需据此排除; 躺卧者全身横卧,
// 髋->膝也与水平近乎平行. 重复候选框的关键点估计有±数像素噪声, 阈值需留足裕量
const LEG_RATIO: f32 = 1.7;
// 第三步: 膝踝连线也须偏横向. 蹲姿/坐地伸腿从高处俯拍时, 躯干前倾+大腿水平与躺卧在
// 2D投影上几乎同形(实测坐姿误报: 躯干33/23, 髋膝25/14 两级全过), 但蹲/坐者小腿竖直
// (踝在膝正下方/斜下方), 躺卧者小腿沿身体横伸(踝与膝同高), 据此区分
const ANKLE_RATIO: f32 = 1.5;
// 关键点尺度守卫: 躯干/髋膝两点距离(640输入坐标系)小于此值说明目标太远太小,
// 关键点估计噪声占主导(实测远处站立者被测出28°倾斜躯干), 比值不可信, 退回框判定
const MIN_TORSO_LEN: f32 = 30.0;
const MIN_LEG_LEN: f32 = 15.0;
const MIN_ANKLE_LEN: f32 = 12.0;
// 第四级: 肩->踝"体轴"须接近水平(躺卧者头脚连线沿地面横伸)
const AXIS_RATIO: f32 = 1.5;
// 第四级: 躯干/大腿/小腿三段与体轴的夹角须小于 ~24°(sin24°≈0.40).
// 躺卧=肩髋膝踝近似一条直线; 坐地伸腿/蹲姿是折线: 实测坐姿膝高于髋和踝
// (低-高-低Z字形), 大腿与体轴夹角33°(sin0.49)被此门拒绝
const COLINEAR_SIN: f32 = 0.40;
// NMS: 同一目标的重复锚框 IoU 超过此值被抑制(各重复框带独立关键点, 是误报放大器)
const NMS_IOU: f32 = 0.5;
// 肩髋关键点都不可用时的兜底: 人形框宽明显超过高(letterbox 等比缩放不改变宽高比).
// 取保守值降低误报(弯腰者的框也可能偏宽)
const LYING_BOX_RATIO: f32 = 1.8;

/// 检测结果: 是否有人 + 是否躺卧姿态
#[derive(Debug, Default, Clone, Copy)]
pub struct DetectionResult {
    pub person: bool,
    pub lying: bool,
}

pub struct PersonDetector {
    model: Option<RunnableModel<TypedFact, Box<dyn TypedOp>, TypedModel>>,
    confidence_threshold: f32,
}

impl PersonDetector {
    /// 创建新的 PersonDetector，如果模型不存在则返回 Err
    pub fn new(model_path: &Path, confidence_threshold: f32) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if !model_path.exists() {
            return Err(format!("模型文件不存在: {}", model_path.display()).into());
        }

        // 加载 ONNX 模型(YOLOv8n, COCO 80类目标检测)
        let model = tract_onnx::onnx().model_for_path(model_path)?;
        let model = model.into_typed()?;
        let model = model.into_optimized()?;
        let model = model.into_runnable()?;

        Ok(Self {
            model: Some(model),
            confidence_threshold,
        })
    }

    /// 检测图片中是否有人, 及是否躺卧姿态
    pub fn detect(
        &mut self,
        image_data: &[u8],
    ) -> Result<DetectionResult, Box<dyn std::error::Error + Send + Sync>> {
        let model = match &self.model {
            Some(m) => m,
            None => return Ok(DetectionResult::default()),
        };

        // 预处理图片
        let input_tensor = self.preprocess_image(image_data)?;

        // 运行推理
        let result = model.run(tvec!(input_tensor.into()))?;

        // 解析输出
        let output = result[0].to_array_view::<f32>()?;
        let (best, count, lying) = parse_detections(&output, self.confidence_threshold);

        let person = best >= self.confidence_threshold;
        if person {
            log_info!(
                "人员检测: 发现{}个人形目标, 最高置信度{:.2}, 躺卧姿态: {}",
                count,
                best,
                if lying { "是" } else { "否" }
            );
        }

        Ok(DetectionResult {
            person,
            lying: lying && person,
        })
    }

    /// 预处理: letterbox 到 640x640(保持宽高比, 灰底114), RGB / 归一化到 0~1, NCHW
    fn preprocess_image(&self, image_data: &[u8]) -> Result<Tensor, Box<dyn std::error::Error + Send + Sync>> {
        let img = image::load_from_memory(image_data)?;
        let (w, h) = img.dimensions();

        // 等比缩放到最长边 640
        let scale = INPUT_SIZE as f32 / w.max(h) as f32;
        let nw = ((w as f32 * scale).round() as u32).clamp(1, INPUT_SIZE);
        let nh = ((h as f32 * scale).round() as u32).clamp(1, INPUT_SIZE);
        let resized = img
            .resize_exact(nw, nh, image::imageops::FilterType::Triangle)
            .to_rgb8();

        // 居中补灰底到正方形(ultralytics letterbox 为居中填充)
        let mut canvas =
            image::RgbImage::from_pixel(INPUT_SIZE, INPUT_SIZE, image::Rgb([114u8, 114, 114]));
        let x_off = (INPUT_SIZE - nw) / 2;
        let y_off = (INPUT_SIZE - nh) / 2;
        image::imageops::overlay(&mut canvas, &resized, x_off, y_off);

        // NCHW 平面布局: 前 640*640 是 R 平面, 依次 G/B(不能按像素交错填充)
        let plane = (INPUT_SIZE * INPUT_SIZE) as usize;
        let mut input = vec![0.0f32; plane * 3];
        for y in 0..INPUT_SIZE as usize {
            for x in 0..INPUT_SIZE as usize {
                let pixel = canvas.get_pixel(x as u32, y as u32);
                let hw = y * INPUT_SIZE as usize + x;
                input[hw] = pixel[0] as f32 / 255.0;
                input[plane + hw] = pixel[1] as f32 / 255.0;
                input[2 * plane + hw] = pixel[2] as f32 / 255.0;
            }
        }

        Tensor::from_shape(&[1, 3, INPUT_SIZE as usize, INPUT_SIZE as usize], &input)
            .map_err(|e| format!("创建输入张量失败: {e}").into())
    }

    /// YOLOv8 没有分类特征向量输出, 返回空(去重自动回退到纯 pHash)
    pub fn extract_feature(&mut self, _image_data: &[u8]) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

/// 解析 YOLOv8 检测输出, 返回(person 最高置信度, 超阈值目标数, 是否存在躺卧姿态)
/// 兼容 [1,84,8400] 纯检测与 [1,56,8400] pose, 以及类别维在前/在后两种布局
fn parse_detections(output: &ArrayViewD<f32>, threshold: f32) -> (f32, usize, bool) {
    let dims = output.shape();
    if dims.len() != 3 || dims[0] != 1 {
        return (0.0, 0, false);
    }

    // 通道数小的一维是类别/关键点维: [1, C, A]
    let (anchors, channels, class_first) = if dims[1] < dims[2] {
        (dims[2], dims[1], true)
    } else {
        (dims[1], dims[2], false)
    };
    let has_pose = channels == POSE_CHANNELS;
    let at = |row: usize, a: usize| -> f32 {
        if class_first {
            output[[0, row, a]]
        } else {
            output[[0, a, row]]
        }
    };

    let mut best = 0.0f32;
    let mut cands: Vec<usize> = Vec::new();
    for a in 0..anchors {
        let score = at(PERSON_SCORE_INDEX, a);
        if score > best {
            best = score;
        }
        if score >= threshold {
            cands.push(a);
        }
    }
    // 贪心 NMS: YOLO 对同一目标输出大量重叠锚框(实测1人产生9框), 各带独立关键点
    // 回归, 任一拼出躺卧形即触发, 是误报放大器; 只保留局部最高分的框参与计数/姿态
    cands.sort_by(|&a, &b| {
        at(PERSON_SCORE_INDEX, b)
            .partial_cmp(&at(PERSON_SCORE_INDEX, a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<usize> = Vec::new();
    for a in cands {
        let overlapped = kept.iter().any(|&b| box_iou(&at, a, b) > NMS_IOU);
        if !overlapped {
            kept.push(a);
        }
    }
    let count = kept.len();
    let lying = has_pose && kept.iter().any(|&a| anchor_is_lying(&at, a, channels));
    (best, count, lying)
}

/// 两个锚框的 IoU(框为中心点+宽高格式)
fn box_iou(at: &dyn Fn(usize, usize) -> f32, a: usize, b: usize) -> f32 {
    let (ax, ay, aw, ah) = (at(0, a), at(1, a), at(2, a), at(3, a));
    let (bx, by, bw, bh) = (at(0, b), at(1, b), at(2, b), at(3, b));
    let (x1, y1) = ((ax - aw / 2.0).max(bx - bw / 2.0), (ay - ah / 2.0).max(by - bh / 2.0));
    let (x2, y2) = ((ax + aw / 2.0).min(bx + bw / 2.0), (ay + ah / 2.0).min(by + bh / 2.0));
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = aw * ah + bw * bh - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// 判断单个候选框是否躺卧: 肩/髋/膝关键点两级判定, 关键点不可用回退框宽高比
/// (坐标为像素或归一化不影响: 横纵分量的比值对等比缩放不变)
/// 第一级: 躯干(肩中点->髋中点)接近水平; 第二级: 髋->膝连线也偏横向(排除弯腰劳作).
/// 实测弯腰俯身时躯干同样接近水平, 只看躯干会把弯腰干活误判成躺卧.
fn anchor_is_lying(at: &dyn Fn(usize, usize) -> f32, a: usize, channels: usize) -> bool {
    let keypoint = |idx: usize| -> Option<(f32, f32)> {
        let base = BOX_VALUES + 1 + idx * 3;
        if base + 2 >= channels {
            return None;
        }
        if at(base + 2, a) < KP_MIN_CONF {
            return None;
        }
        Some((at(base, a), at(base + 1, a)))
    };
    // 成对关键点取中点; 一侧被遮挡时退用可见一侧(髋/膝单侧可见很常见)
    let pair_mid = |i1: usize, i2: usize| -> Option<(f32, f32)> {
        match (keypoint(i1), keypoint(i2)) {
            (Some(p), Some(q)) => Some(((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0)),
            (Some(p), None) | (None, Some(p)) => Some(p),
            (None, None) => None,
        }
    };
    let shoulders = pair_mid(KP_SHOULDER_L, KP_SHOULDER_R);
    let hips = pair_mid(KP_HIP_L, KP_HIP_R);
    if let (Some(s), Some(h)) = (shoulders, hips) {
        let (dx, dy) = ((h.0 - s.0).abs(), (h.1 - s.1).abs());
        if dx <= dy * TORSO_RATIO {
            return false; // 躯干不水平: 站立/坐姿/蹲姿
        }
        // 躺卧时全身横卧, 髋->膝连线也接近水平; 弯腰/弓步时腿竖直或斜向
        if let Some(k) = pair_mid(KP_KNEE_L, KP_KNEE_R) {
            let (ldx, ldy) = ((k.0 - h.0).abs(), (k.1 - h.1).abs());
            let torso_len = (dx * dx + dy * dy).sqrt();
            let leg_len = (ldx * ldx + ldy * ldy).sqrt();
            // 目标太小时关键点两点距离只有十几像素, ±数像素噪声主导比值判定
            // (实测远处站立者被测出28°倾斜躯干), 尺度不足不采信, 退回框判定
            if torso_len >= MIN_TORSO_LEN && leg_len >= MIN_LEG_LEN {
                if ldx <= ldy * LEG_RATIO {
                    return false; // 腿竖直/斜向: 弯腰/站立/弓步
                }
                // 第三级: 小腿(膝->踝)也须接近水平. 蹲姿/坐地伸腿从高处俯拍时
                // 躯干+大腿两级全过(实测坐姿误报), 但其小腿竖直(踝在膝下方),
                // 躺卧者小腿沿身体横伸(踝与膝同高), 据此区分
                if let Some(an) = pair_mid(KP_ANKLE_L, KP_ANKLE_R) {
                    let (adx, ady) = ((an.0 - k.0).abs(), (an.1 - k.1).abs());
                    let ankle_len = (adx * adx + ady * ady).sqrt();
                    if ankle_len >= MIN_ANKLE_LEN {
                        if adx <= ady * ANKLE_RATIO {
                            return false; // 小腿竖直: 蹲/坐姿
                        }
                        // 第四级: 共线判定(决定性). 躺卧者肩髋膝踝近似一条直线;
                        // 坐地伸腿是折线(膝凸起: 低-高-低), 逐段 abs 比值会漏判,
                        // 须验证三段与"肩->踝体轴"的夹角都足够小
                        let (ax, ay) = (an.0 - s.0, an.1 - s.1);
                        if ax.abs() <= ay.abs() * AXIS_RATIO {
                            return false; // 体轴不接近水平: 站/坐/蹲
                        }
                        let axis_len = (ax * ax + ay * ay).sqrt();
                        let segments = [
                            (h.0 - s.0, h.1 - s.1), // 躯干
                            (k.0 - h.0, k.1 - h.1), // 大腿
                            (an.0 - k.0, an.1 - k.1), // 小腿
                        ];
                        for (sx, sy) in segments {
                            let cross = (ax * sy - ay * sx).abs();
                            let len = (sx * sx + sy * sy).sqrt();
                            if cross > len * axis_len * COLINEAR_SIN {
                                return false; // 该段偏离体轴: 身体折叠(坐/蹲)
                            }
                        }
                        log_info!(
                            "躺卧判定(关键点): 躯干横纵 {:.0}/{:.0}, 髋膝横纵 {:.0}/{:.0}, 膝踝横纵 {:.0}/{:.0}, 体轴横纵 {:.0}/{:.0}",
                            dx, dy, ldx, ldy, adx, ady, ax, ay
                        );
                        return true;
                    }
                }
                // 踝关键点不可用或尺度不足: 不据关键点定论, 落到框宽高比判定
            }
        }
        // 膝关键点不可用: 退回框宽高比确认(腿被遮挡的躺卧者整框仍明显偏宽;
        // 弯腰者的框包含竖直的腿, 不会明显偏宽)
    }
    // 兜底: 人形框宽高比
    let (w, h) = (at(2, a), at(3, a));
    if w > h * LYING_BOX_RATIO {
        log_info!("躺卧判定(框宽高比): 框宽高 {:.0}/{:.0}", w, h);
        return true;
    }
    false
}


/// 从 URL 下载图片并提取特征向量

#[cfg(test)]
mod tests {
    use super::*;
    use tract_onnx::prelude::tract_ndarray::Array;

    /// 构造 [1, channels, anchors] 布局的模型输出张量
    /// (anchors 用真实规模 8400, 通道维判定逻辑依赖 anchors >> channels)
    struct OutputBuilder {
        channels: usize,
        anchors: usize,
        anchor: usize,
        data: Vec<f32>,
    }

    impl OutputBuilder {
        fn new(channels: usize, anchors: usize, anchor: usize) -> Self {
            Self {
                channels,
                anchors,
                anchor,
                data: vec![0.0; channels * anchors],
            }
        }
        fn set(&mut self, row: usize, v: f32) -> &mut Self {
            self.data[row * self.anchors + self.anchor] = v;
            self
        }
        fn person(&mut self, score: f32) -> &mut Self {
            self.set(PERSON_SCORE_INDEX, score)
        }
        fn box_wh(&mut self, w: f32, h: f32) -> &mut Self {
            self.set(2, w).set(3, h)
        }
        fn keypoint(&mut self, idx: usize, x: f32, y: f32, conf: f32) -> &mut Self {
            let base = BOX_VALUES + 1 + idx * 3;
            self.set(base, x).set(base + 1, y).set(base + 2, conf)
        }
        fn parse(&self, threshold: f32) -> (f32, usize, bool) {
            let arr =
                Array::from_shape_vec((1, self.channels, self.anchors), self.data.clone())
                    .unwrap();
            parse_detections(&arr.view().into_dyn(), threshold)
        }
    }

    #[test]
    fn pose_output_horizontal_torso_is_lying() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(500.0, 100.0);
        // 躺卧: 肩(105,101) 髋(205,102) 膝(305,102) 踝(405,102), 全身横向
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 110.0, 102.0, 0.8)
            .keypoint(KP_HIP_L, 200.0, 101.0, 0.8)
            .keypoint(KP_HIP_R, 210.0, 103.0, 0.8)
            .keypoint(KP_KNEE_L, 300.0, 101.0, 0.8)
            .keypoint(KP_KNEE_R, 310.0, 103.0, 0.8)
            .keypoint(KP_ANKLE_L, 400.0, 101.0, 0.8)
            .keypoint(KP_ANKLE_R, 410.0, 103.0, 0.8);
        let (best, count, lying) = b.parse(0.5);
        assert!(best >= 0.9 && count == 1);
        assert!(lying, "躯干/大腿/小腿都横向应判为躺卧");
    }

    /// 坐地伸腿(高处俯拍, 2026-11-18 实际误报): 躯干前倾投影偏横向, 大腿前伸投影水平,
    /// 但小腿向下(踝在膝下方), 不得判躺卧
    #[test]
    fn pose_sitting_legs_forward_not_lying() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(150.0, 120.0);
        // 躯干 dx/dy=2.2 过躯干门; 大腿 dx/dy=2.5 过腿门; 踝(208,240)在膝(205,200)正下方
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 110.0, 102.0, 0.8)
            .keypoint(KP_HIP_L, 200.0, 140.0, 0.8)
            .keypoint(KP_HIP_R, 210.0, 142.0, 0.8)
            .keypoint(KP_KNEE_L, 250.0, 150.0, 0.8)
            .keypoint(KP_KNEE_R, 260.0, 152.0, 0.8)
            .keypoint(KP_ANKLE_L, 205.0, 240.0, 0.8)
            .keypoint(KP_ANKLE_R, 215.0, 242.0, 0.8);
        let (_, _, lying) = b.parse(0.5);
        assert!(!lying, "坐地伸腿(踝在膝下方)不应判为躺卧");
    }

    /// 坐地抱盆(2026-11-18 实测触发的真实坐标, 高处俯拍): 膝(424)高于髋(435)和踝(431),
    /// 腿呈低-高-低 Z 字折线, 三段逐段看都"接近水平"(逐段 abs 比值全部通过),
    /// 只有共线判定能识别出身体是折叠的而非一条直线
    #[test]
    fn pose_sitting_zigzag_legs_colinearity_not_lying() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.81).box_wh(124.0, 74.0);
        // 实测: 肩(100,414) 髋(132,435) 膝(161,424) 踝(176,431)
        b.keypoint(KP_SHOULDER_L, 98.0, 412.0, 0.95)
            .keypoint(KP_SHOULDER_R, 102.0, 416.0, 0.97)
            .keypoint(KP_HIP_L, 131.0, 433.0, 0.95)
            .keypoint(KP_HIP_R, 134.0, 437.0, 0.97)
            .keypoint(KP_KNEE_L, 158.0, 423.0, 0.91)
            .keypoint(KP_KNEE_R, 165.0, 425.0, 0.95)
            .keypoint(KP_ANKLE_L, 172.0, 430.0, 0.87)
            .keypoint(KP_ANKLE_R, 181.0, 432.0, 0.91);
        let (_, _, lying) = b.parse(0.5);
        assert!(!lying, "坐姿Z字形折线(膝凸起)即使逐段全水平也不应判躺卧");
    }

    /// 蹲姿(2026-11-18 实测数值: 躯干横纵33/23): 投影倾角35°不算接近水平,
    /// 躯干门(1.5)应直接拒绝
    #[test]
    fn pose_squat_tilted_torso_not_lying() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(150.0, 120.0);
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 101.0, 101.0, 0.8)
            .keypoint(KP_HIP_L, 130.0, 122.0, 0.8)
            .keypoint(KP_HIP_R, 131.0, 123.0, 0.8)
            .keypoint(KP_KNEE_L, 152.0, 134.0, 0.8)
            .keypoint(KP_KNEE_R, 153.0, 135.0, 0.8);
        let (_, _, lying) = b.parse(0.5);
        assert!(!lying, "躯干横纵33/23(35°倾角)是蹲/坐前倾, 不应判为躺卧");
    }

    /// 躺卧者踝被遮挡(置信度不足): 关键点不定论, 退框宽高比; 宽框(400x100)应兜底判躺卧
    #[test]
    fn pose_lying_ankles_hidden_wide_box_fallback() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(400.0, 100.0);
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 110.0, 102.0, 0.8)
            .keypoint(KP_HIP_L, 200.0, 101.0, 0.8)
            .keypoint(KP_HIP_R, 210.0, 103.0, 0.8)
            .keypoint(KP_KNEE_L, 300.0, 101.0, 0.8)
            .keypoint(KP_KNEE_R, 310.0, 103.0, 0.8)
            .keypoint(KP_ANKLE_L, 400.0, 101.0, 0.1);
        let (_, _, lying) = b.parse(0.5);
        assert!(lying, "躯干/腿横向+踝不可见+宽框应兜底判为躺卧");
    }

    /// 弯腰劳作: 躯干接近水平但腿保持竖直(膝在髋正下方), 不应判躺卧(真实误报场景)
    #[test]
    fn pose_bent_over_horizontal_torso_vertical_legs_not_lying() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(150.0, 200.0);
        // 肩(105,101) 髋(205,102) 躯干横向; 膝(208,205) 在髋正下方, 腿竖直
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 110.0, 102.0, 0.8)
            .keypoint(KP_HIP_L, 200.0, 101.0, 0.8)
            .keypoint(KP_HIP_R, 210.0, 103.0, 0.8)
            .keypoint(KP_KNEE_L, 203.0, 200.0, 0.8)
            .keypoint(KP_KNEE_R, 213.0, 210.0, 0.8);
        let (_, _, lying) = b.parse(0.5);
        assert!(!lying, "弯腰(躯干水平+腿竖直)不应判为躺卧");
    }

    #[test]
    fn pose_tiny_person_keypoints_not_trusted() {
        // 远处小目标: 关键点只有十几像素, 比值被噪声主导(实测站立者被测出
        // 28°倾斜躯干). 尺度不足时不得按关键点判躺卧, 退回框宽高比(高框→非躺卧)
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(30.0, 80.0);
        // 肩(100,100) 髋(120,110): 躯干横向但仅21px; 膝(130,113) 也横向仅10px
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 102.0, 101.0, 0.8)
            .keypoint(KP_HIP_L, 120.0, 110.0, 0.8)
            .keypoint(KP_HIP_R, 122.0, 111.0, 0.8)
            .keypoint(KP_KNEE_L, 130.0, 112.0, 0.8)
            .keypoint(KP_KNEE_R, 132.0, 113.0, 0.8);
        let (_, _, lying) = b.parse(0.5);
        assert!(!lying, "小目标关键点比值不可信, 高框不应判为躺卧");
    }

    #[test]
    fn pose_output_vertical_torso_not_lying() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        b.person(0.9).box_wh(60.0, 200.0);
        // 肩(105,101) 髋(105,202): 躯干纵向(站立/坐姿)
        b.keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.8)
            .keypoint(KP_SHOULDER_R, 110.0, 102.0, 0.8)
            .keypoint(KP_HIP_L, 103.0, 200.0, 0.8)
            .keypoint(KP_HIP_R, 107.0, 205.0, 0.8);
        let (_, _, lying) = b.parse(0.5);
        assert!(!lying, "纵向躯干不应判为躺卧");
    }

    #[test]
    fn pose_weak_keypoints_fallback_to_box_ratio() {
        let mut b = OutputBuilder::new(POSE_CHANNELS, 8400, 100);
        // 关键点置信度不足, 走框宽高比兜底: 宽300 高100
        b.person(0.9)
            .box_wh(300.0, 100.0)
            .keypoint(KP_SHOULDER_L, 100.0, 100.0, 0.1);
        let (_, _, lying) = b.parse(0.5);
        assert!(lying, "关键点弱时宽框应兜底判为躺卧");
    }

    #[test]
    fn detect_output_84_channels_reports_person_only() {
        let mut b = OutputBuilder::new(84, 8400, 100);
        b.person(0.9).box_wh(300.0, 100.0);
        let (best, count, lying) = b.parse(0.5);
        assert!(best >= 0.9 && count == 1);
        assert!(!lying, "纯检测模型(84通道)不输出姿态");
    }

    /// 标准测试图(公交车上多人, ultralytics 官方样例)必须检出人且不误报躺卧
    #[test]
    fn pose_detects_person_in_sample() {
        let model = std::path::Path::new("models/yolov8n-pose.onnx");
        let img_path = std::path::Path::new("test_assets/bus.jpg");
        if !model.exists() || !img_path.exists() {
            log_error!("跳过: 缺少 models/yolov8n-pose.onnx 或 test_assets/bus.jpg");
            return;
        }
        let mut det = PersonDetector::new(model, 0.5).expect("模型加载失败");
        let img = std::fs::read(img_path).unwrap();
        let r = det.detect(&img).expect("推理失败");
        assert!(r.person, "bus.jpg 应检测到人");
        assert!(!r.lying, "bus.jpg 站/坐姿不应判为躺卧");
    }

    /// 把 bus.jpg 旋转90度: 验证分布外输入下 person 检测仍存活(置信度~0.5, 阈值取0.3).
    /// 不再断言躺卧: 实测旋转属分布外输入, 关键点输出不可靠(会出现"横向躯干+竖直腿"
    /// 的几何矛盾), 躺卧正例由合成关键点单测覆盖(pose_output_horizontal_torso_is_lying)
    #[test]
    fn pose_detects_person_in_rotated_sample() {
        let model = std::path::Path::new("models/yolov8n-pose.onnx");
        let img_path = std::path::Path::new("test_assets/bus.jpg");
        if !model.exists() || !img_path.exists() {
            log_error!("跳过: 缺少 models/yolov8n-pose.onnx 或 test_assets/bus.jpg");
            return;
        }
        let mut det = PersonDetector::new(model, 0.3).expect("模型加载失败");
        let img = image::open(img_path).unwrap().rotate90();
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageOutputFormat::Png).unwrap();
        let r = det.detect(buf.get_ref()).expect("推理失败");
        assert!(r.person, "旋转后仍应检测到人");
    }

    /// 误报回归: 服务器实际触发过躺卧提醒的4张图(3张老人弯腰劳作 2026-11-17/18 +
    /// 1张老人坐墙根抱盆 2026-11-18, 高处俯拍坐姿与躺卧2D同形), 修复后不得再判躺卧
    #[test]
    fn real_bent_over_images_not_lying() {
        let model = std::path::Path::new("models/yolov8n-pose.onnx");
        if !model.exists() {
            log_error!("跳过: 缺少 models/yolov8n-pose.onnx");
            return;
        }
        for name in ["bent_over.jpg", "bent_over2.jpg", "bent_over3.jpg", "sitting_basin.jpg"] {
            let img_path = std::path::Path::new("test_assets").join(name);
            if !img_path.exists() {
                log_error!("跳过: 缺少 {}", img_path.display());
                continue;
            }
            let mut det = PersonDetector::new(model, 0.4).expect("模型加载失败");
            let img = std::fs::read(&img_path).unwrap();
            let r = det.detect(&img).expect("推理失败");
            log_info!("{} => person: {}, lying: {}", name, r.person, r.lying);
            assert!(!r.lying, "{} 实为弯腰劳作, 不应判为躺卧", name);
        }
    }

    /// 真实摄像头抓拍图, 观察检测结果(不设硬断言, 场景是否有人不确定)
    #[test]
    fn pose_runs_on_camera_capture() {
        let model = std::path::Path::new("models/yolov8n-pose.onnx");
        let img_path = std::path::Path::new("test_assets/scene.jpg");
        if !model.exists() || !img_path.exists() {
            log_error!("跳过: 缺少 models/yolov8n-pose.onnx 或 test_assets/scene.jpg");
            return;
        }
        let mut det = PersonDetector::new(model, 0.5).expect("模型加载失败");
        let img = std::fs::read(img_path).unwrap();
        let r = det.detect(&img).expect("推理失败");
        log_info!("scene.jpg 检测到人: {}, 躺卧: {}", r.person, r.lying);
    }

    /// FP 诊断探针: 打印指定图中 person 分数最高的若干候选框的框/关键点,
    /// 用于核对是哪个框触发了躺卧判定(本地诊断用, 不设断言)
    #[test]
    fn probe_dump_top_anchors() {
        let model = std::path::Path::new("models/yolov8n-pose.onnx");
        let probe_img =
            std::env::var("PROBE_IMG").unwrap_or_else(|_| "test_assets/sitting_basin.jpg".into());
        let img_path = std::path::Path::new(&probe_img);
        if !model.exists() || !img_path.exists() {
            log_error!("跳过: 缺少 {} 或 {}", model.display(), img_path.display());
            return;
        }
        let det = PersonDetector::new(model, 0.4).expect("模型加载失败");
        let tensor = det
            .preprocess_image(&std::fs::read(&img_path).unwrap())
            .expect("预处理失败");
        let result = det
            .model
            .as_ref()
            .unwrap()
            .run(tvec!(tensor.into()))
            .expect("推理失败");
        let output = result[0].to_array_view::<f32>().unwrap();
        let dims = output.shape();
        let (anchors, class_first) = if dims[1] < dims[2] {
            (dims[2], true)
        } else {
            (dims[1], false)
        };
        let at = |row: usize, a: usize| -> f32 {
            if class_first {
                output[[0, row, a]]
            } else {
                output[[0, a, row]]
            }
        };
        let mut idx: Vec<usize> = (0..anchors)
            .filter(|&a| at(PERSON_SCORE_INDEX, a) >= 0.4)
            .collect();
        idx.sort_by(|&a, &b| {
            at(PERSON_SCORE_INDEX, b)
                .partial_cmp(&at(PERSON_SCORE_INDEX, a))
                .unwrap()
        });
        for (rank, &a) in idx.iter().take(8).enumerate() {
            let kp = |i: usize| {
                let base = BOX_VALUES + 1 + i * 3;
                format!("({:.0},{:.0},{:.2})", at(base, a), at(base + 1, a), at(base + 2, a))
            };
            log_info!(
                "#{} score={:.2} box=({:.0},{:.0},{:.0},{:.0}) 肩L{} 肩R{} 髋L{} 髋R{} 膝L{} 膝R{} 踝L{} 踝R{} 鼻{}",
                rank, at(PERSON_SCORE_INDEX, a), at(0, a), at(1, a), at(2, a), at(3, a),
                kp(5), kp(6), kp(11), kp(12), kp(13), kp(14), kp(15), kp(16), kp(0)
            );
        }
    }
}
