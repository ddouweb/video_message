use std::path::Path;
use image::GenericImageView;
use std::sync::Mutex;
use tract_onnx::prelude::*;
use tract_onnx::prelude::tract_ndarray::ArrayViewD;

// YOLOv8n 输入尺寸(ultralytics 默认导出 640x640)
const INPUT_SIZE: u32 = 640;
// COCO 数据集 80 类, 第 0 类 = person
const PERSON_CLASS: usize = 0;
// 每个候选框前 4 个值是坐标(cx,cy,w,h), 第 5 个值起是各类别得分
const BOX_VALUES: usize = 4;

pub struct PersonDetector {
    model: Option<RunnableModel<TypedFact, Box<dyn TypedOp>, TypedModel>>,
    confidence_threshold: f32,
    has_sent_alert: Mutex<bool>,
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
            has_sent_alert: Mutex::new(false),
        })
    }

    /// 检测图片中是否有人
    pub fn detect(&mut self, image_data: &[u8]) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let model = match &self.model {
            Some(m) => m,
            None => return Ok(false),
        };

        // 预处理图片
        let input_tensor = self.preprocess_image(image_data)?;

        // 运行推理
        let result = model.run(tvec!(input_tensor.into()))?;

        // 解析输出
        let output = result[0].to_array_view::<f32>()?;
        let (best, count) = parse_person_scores(&output, self.confidence_threshold);

        if best >= self.confidence_threshold {
            println!("人员检测: 发现{}个人形目标, 最高置信度{:.2}", count, best);
            *self.has_sent_alert.lock().unwrap() = true;
        }

        Ok(best >= self.confidence_threshold)
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

    /// 获取是否已发送过提醒
    pub fn has_sent_alert(&self) -> bool {
        *self.has_sent_alert.lock().unwrap()
    }

    /// 重置提醒状态
    pub fn reset(&mut self) {
        *self.has_sent_alert.lock().unwrap() = false;
    }

    /// YOLOv8 没有分类特征向量输出, 返回空(去重自动回退到纯 pHash)
    pub fn extract_feature(&mut self, _image_data: &[u8]) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

/// 解析 YOLOv8 检测输出, 返回(person 类最高置信度, 超过阈值的候选框数)
/// 兼容 [1,84,8400](pytorch 导出) 和 [1,8400,84](TF 导出) 两种布局
fn parse_person_scores(output: &ArrayViewD<f32>, threshold: f32) -> (f32, usize) {
    let dims = output.shape();
    if dims.len() != 3 || dims[0] != 1 {
        return (0.0, 0);
    }

    // 类别维(84)在前 → [1, 84, 8400]; 否则是 [1, 8400, 84]
    let (anchors, class_first) = if dims[1] < dims[2] {
        (dims[2], true)
    } else {
        (dims[1], false)
    };
    let score_index = BOX_VALUES + PERSON_CLASS;

    let mut best = 0.0f32;
    let mut count = 0usize;
    for a in 0..anchors {
        let score = if class_first {
            output[[0, score_index, a]]
        } else {
            output[[0, a, score_index]]
        };
        if score > best {
            best = score;
        }
        if score >= threshold {
            count += 1;
        }
    }
    (best, count)
}

/// 从 URL 下载图片并检测是否有人
pub async fn detect_person_from_url(
    client: &reqwest::Client,
    url: &str,
    detector: &mut PersonDetector,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    detector.detect(&bytes)
}

/// 从 URL 下载图片并提取特征向量
pub async fn extract_feature_from_url(
    client: &reqwest::Client,
    url: &str,
    detector: &mut PersonDetector,
) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    detector.extract_feature(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准测试图(公交车上多人, ultralytics 官方样例)必须检出人
    #[test]
    fn yolov8n_detects_person_in_sample() {
        let model = std::path::Path::new("models/yolov8n.onnx");
        let img_path = std::path::Path::new("test_assets/bus.jpg");
        if !model.exists() || !img_path.exists() {
            eprintln!("跳过: 缺少 models/yolov8n.onnx 或 test_assets/bus.jpg");
            return;
        }
        let mut det = PersonDetector::new(model, 0.5).expect("模型加载失败");
        let img = std::fs::read(img_path).unwrap();
        assert!(det.detect(&img).expect("推理失败"), "bus.jpg 应检测到人");
    }

    /// 真实摄像头抓拍图, 观察检测结果(不设硬断言, 场景是否有人不确定)
    #[test]
    fn yolov8n_runs_on_camera_capture() {
        let model = std::path::Path::new("models/yolov8n.onnx");
        let img_path = std::path::Path::new("test_assets/scene.jpg");
        if !model.exists() || !img_path.exists() {
            eprintln!("跳过: 缺少 models/yolov8n.onnx 或 test_assets/scene.jpg");
            return;
        }
        let mut det = PersonDetector::new(model, 0.5).expect("模型加载失败");
        let img = std::fs::read(img_path).unwrap();
        let found = det.detect(&img).expect("推理失败");
        println!("scene.jpg 检测到人: {found}");
    }
}
