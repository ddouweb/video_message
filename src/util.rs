pub async fn send_post_request(client:&reqwest::Client,body: String) -> reqwest::Result<serde_json::Value> {
    //println!("debug:已经发起网络请求:{body}");
    let url = "http://www.pushplus.plus/send";
    let response = client
        .post(url)
        .timeout(std::time::Duration::from_secs(3))
        .header("User-Agent", "rust_video")
        .header("Content-Type", "application/json")
        .body(body.to_owned())
        .send()
        .await?;

    let response_text =response.json().await.unwrap();   
    //println!("debug:网络响应:{response_text}");
    Ok(response_text)
}