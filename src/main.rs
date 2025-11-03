use anyhow::{anyhow, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client,
};
use serde_json::Value;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    checkin().await?;
    Ok(())
}

async fn checkin() -> Result<()> {
    let cookie = fs::read_to_string("cookie.txt")?.trim().to_string();

    if cookie.is_empty() {
        return Err(anyhow!("cookie.txt is empty or not found"));
    }

    let mut headers = HeaderMap::new();

    headers.insert("Host", HeaderValue::from_static("okti.xyz"));
    headers.insert(
        "User-Agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:145.0) Gecko/20100101 Firefox/145.0",
        ),
    );
    headers.insert(
        "Accept",
        HeaderValue::from_static("application/json, text/javascript, */*; q=0.01"),
    );
    headers.insert(
        "Accept-Language",
        HeaderValue::from_static("zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2"),
    );
    headers.insert(
        "X-Requested-With",
        HeaderValue::from_static("XMLHttpRequest"),
    );
    headers.insert("Origin", HeaderValue::from_static("https://okti.xyz"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("Referer", HeaderValue::from_static("https://okti.xyz/user"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("Content-Length", HeaderValue::from_static("0"));
    headers.insert("TE", HeaderValue::from_static("trailers"));

    // 设置 Cookie
    headers.insert("Cookie", HeaderValue::from_str(&cookie)?);

    let client = Client::builder().default_headers(headers).build()?;

    let url = "https://okti.xyz/user/checkin";
    let res = client.post(url).send().await?;

    println!("Status: {}", res.status());

    // // 5. 尝试解析 JSON 并 pretty-print
    match res.json::<Value>().await {
        Ok(json_value) => {
            println!("Response JSON:");
            println!("{}", serde_json::to_string_pretty(&json_value)?);
        }
        Err(e) => {
            eprintln!("Failed to parse response as JSON: {}", e);
        }
    }

    Ok(())
}
