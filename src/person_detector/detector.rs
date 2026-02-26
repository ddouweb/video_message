use std::path::Path;
use image::GenericImageView;
use std::sync::Mutex;
use tract_onnx::prelude::*;
use tract_ndarray::ArrayViewD;

pub struct PersonDetector {
    model: Option<TypedModel>,
    input_name: String,
    output_name: String,
    confidence_threshold: f32,
    has_sent_alert: Mutex<bool>,
}

impl PersonDetector {
    /// 创建新的 PersonDetector，如果模型不存在则返回 Err
    pub fn new(model_path: &Path, confidence_threshold: f32) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if !model_path.exists() {
            return Err("模型文件不存在: models/mobilenetv2.onnx".into());
        }

        // 加载 ONNX 模型
        let model = tract_onnx::onnx::onnx().load_file(model_path)?;
        let model = model.into_optimized()?;
        let model = model.into_runnable()?;

        // 获取输入输出信息
        let input_name = model.input_names()[0].0.clone();
        let output_name = model.output_names()[0].0.clone();

        Ok(Self {
            model: Some(model),
            input_name,
            output_name,
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
        let result = model.run(tvec!(input_tensor))?;

        // 解析输出
        let output = result[0].to_array_view()?;
        let result = self.parse_output(&output)?;

        // 如果检测到人且未发送过提醒
        if result {
            let mut has_sent = self.has_sent_alert.lock().unwrap();
            if !*has_sent {
                *has_sent = true;
            }
        }

        Ok(result)
    }

    /// 预处理图片为模型输入
    fn preprocess_image(&self, image_data: &[u8]) -> Result<Tensor, Box<dyn std::error::Error + Send + Sync>> {
        // 解码图片
        let img = image::load_from_memory(image_data)?;

        // 调整大小为 224x224 (MobileNet V2 输入尺寸)
        let img = img.resize_exact(224, 224, image::imageops::FilterType::Nearest);

        // 转换为 RGB 数组并归一化
        let mut input = vec![0.0f32; 3 * 224 * 224];
        for y in 0..224 {
            for x in 0..224 {
                let pixel = img.get_pixel(x, y);
                let idx = (y * 224 + x) * 3;
                // ImageNet 归一化
                input[idx] = (pixel[0] as f32 - 124.0) / 255.0;
                input[idx + 1] = (pixel[1] as f32 - 117.0) / 255.0;
                input[idx + 2] = (pixel[2] as f32 - 104.0) / 255.0;
            }
        }

        // 创建 4D tensor [1, 3, 224, 224]
        let tensor = Tensor::from_shape(&[1, 3, 224, 224], &input)?;
        Ok(tensor)
    }

    /// 解析模型输出
    fn parse_output(&self, output: &ArrayViewD<f32>) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let mut max_confidence = 0.0f32;
        let mut max_class = 0usize;

        // 找到最高置信度的类别
        for (i, confidence) in output.iter().enumerate() {
            if *confidence > max_confidence {
                max_confidence = *confidence;
                max_class = i;
            }
        }

        // ImageNet 类别中，"person" 类别 ID 为 281
        // MobileNet V2: 281 = person
        let person_class = 281;

        if max_class == person_class && max_confidence >= self.confidence_threshold {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 获取是否已发送过提醒
    pub fn has_sent_alert(&self) -> bool {
        *self.has_sent_alert.lock().unwrap()
    }

    /// 重置提醒状态
    pub fn reset(&mut self) {
        *self.has_sent_alert.lock().unwrap() = false;
    }

    /// 提取图片的特征向量（用于去重）
    pub fn extract_feature(&mut self, image_data: &[u8]) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
        let model = match &self.model {
            Some(m) => m,
            None => return Ok(vec![]),
        };

        // 预处理图片
        let input_tensor = self.preprocess_image(image_data)?;

        // 运行推理，获取特征向量
        let result = model.run(tvec!(input_tensor))?;

        // 获取输出
        let output = result[0].to_array_view()?;

        // MobileNet V2 输出是 [1, 1280]，提取特征向量
        let mut feature = Vec::with_capacity(1280);
        for i in 0..1280 {
            feature.push(output[[0, i]]);
        }

        Ok(feature)
    }
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
