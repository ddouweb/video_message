// use reqwest::{Client,Result};
// use std::error::Error;

// impl HttpClient {
//     pub fn new() -> HttpClient {
//         let client = Client::builder()
//             .user_agent("rust-video")
//             .build()
//             .expect("Failed to build HttpClient");

//         HttpClient { client }
//     }

//     pub async fn send_post_request(&self, url: &str, body: &str) -> Result<String> {
//         let response = self
//             .client
//             .post(url)
//             .header("User-Agent", "rust_video")
//             .header("Content-Type", "application/json")
//             .body(body.to_owned())
//             .send()
//             .await?;

//         let response_text = response.text().await?;

//         Ok(response_text)
//     }
// }
