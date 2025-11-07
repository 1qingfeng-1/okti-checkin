use anyhow::{self, Context};
use chrono::{DateTime, Utc};
use headless_chrome::{Browser, LaunchOptions};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use std::{collections::HashMap, str::FromStr};
use tokio::{fs, task};
use tracing::{debug, info, warn};

const TARGET_URL: &str = "okti.xyz";
const COOKIE_FILE: &str = "cookie.txt";
const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:145.0) Gecko/20100101 Firefox/145.0";
const CF_CLEARANCE_NAME: &str = "cf_clearance"; // 只提取特定cookie，避免全量
const IP_PLACEHOLDER: &str = "12704efe9702be1480f07823cef5222b"; // IP MD5
const COOKIE_EXPIRE_DAYS: i64 = 30;
const TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize, Serialize)]
struct JsonResult {
    ret: i32,
    msg: String,
}

pub struct OktiXyz {
    email: String,
    passwd: String,
}

impl OktiXyz {
    pub fn new(email: String, passwd: String) -> Self {
        Self { email, passwd }
    }

    async fn get_cf_clearance(&self) -> anyhow::Result<String> {
        let base_url = format!("https://{}", TARGET_URL);
        // 阻塞线程执行Chrome操作
        let cf_cookie = task::spawn_blocking(move || -> anyhow::Result<String> {
            // 构建共享的 LaunchOptions builder
            let mut builder = LaunchOptions::default_builder(); // owned LaunchOptionsBuilder
            builder.headless(true); // &mut self -> &mut Self，变异就地
            builder.sandbox(false); // 禁用沙箱，适合服务器
            builder.enable_gpu(false);

            #[cfg(target_os = "linux")]
            {
                builder.path(Some("/usr/bin/chromium".into()));
            }

            let options = builder.build().context("Failed to build Chrome options")?;

            let browser = Browser::new(options).context("Failed to launch Chrome browser")?;
            let tab = browser.new_tab().context("Failed to create tab")?;

            tab.navigate_to(&base_url)
                .context("Failed to navigate to URL")?;
            tab.wait_until_navigated()?;
            // 轮询等待 cf_clearance（带超时）
            let start = Instant::now();
            let mut clearance_cookie: Option<String> = None;
            while start.elapsed() < TIMEOUT {
                let cookies = tab.get_cookies().context("Failed to get cookies")?;
                if let Some(cookie) = cookies.into_iter().find(|c| c.name == CF_CLEARANCE_NAME) {
                    clearance_cookie = Some(format!("{}={}", cookie.name, cookie.value));
                    break;
                }
                std::thread::sleep(POLL_INTERVAL); // 轮询间隔
            }

            if let Some(cf) = clearance_cookie {
                // 如果需要添加ip/expire，动态计算
                let now: DateTime<Utc> = Utc::now();
                let expire = now + chrono::Duration::days(COOKIE_EXPIRE_DAYS);
                let mut parts = Vec::with_capacity(3);
                parts.push(cf.clone());
                parts.push(format!("ip={}", IP_PLACEHOLDER));
                parts.push(format!("expire_in={}", expire.timestamp()));
                Ok(parts.join("; "))
            } else {
                Err(anyhow::anyhow!("cf_clearance cookie not found"))
            }
        })
        .await
        .context("Chrome task failed")??;

        debug!(
            "Retrieved cf_clearance: {}",
            &cf_cookie[..cf_cookie.find(';').unwrap_or(cf_cookie.len())]
        ); // 日志截断敏感部分
        Ok(cf_cookie)
    }

    fn build_base_request_headers(
        &self,
        extra: Option<HashMap<String, String>>, // owned Strings，避免借用
    ) -> anyhow::Result<HeaderMap> {
        let mut headers = HeaderMap::new();

        // 静态 common headers：用 from_static + HeaderName::from_static
        let common = [
            (
                HeaderName::from_static("user-agent"),
                HeaderValue::from_static(DEFAULT_USER_AGENT),
            ),
            (
                HeaderName::from_static("accept"),
                HeaderValue::from_static("application/json, text/javascript, */*; q=0.01"),
            ),
            (
                HeaderName::from_static("accept-language"),
                HeaderValue::from_static(
                    "zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2",
                ),
            ),
            (
                HeaderName::from_static("x-requested-with"),
                HeaderValue::from_static("XMLHttpRequest"),
            ),
            (
                HeaderName::from_static("host"),
                HeaderValue::from_static(TARGET_URL),
            ),
            (
                HeaderName::from_static("connection"),
                HeaderValue::from_static("keep-alive"),
            ),
            (
                HeaderName::from_static("sec-fetch-dest"),
                HeaderValue::from_static("empty"),
            ),
            (
                HeaderName::from_static("sec-fetch-mode"),
                HeaderValue::from_static("cors"),
            ),
            (
                HeaderName::from_static("sec-fetch-site"),
                HeaderValue::from_static("same-origin"),
            ),
            (
                HeaderName::from_static("priority"),
                HeaderValue::from_static("u=0"),
            ),
            (
                HeaderName::from_static("pragma"),
                HeaderValue::from_static("no-cache"),
            ),
            (
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static("no-cache"),
            ),
        ];

        for (name, value) in common {
            headers.insert(name, value);
        }

        // 动态 Origin
        let base_url = format!("https://{}/", TARGET_URL);
        headers.insert(
            HeaderName::from_str("origin")?,
            HeaderValue::from_str(&base_url)?,
        );

        // 动态 extra headers：从 owned String 创建 HeaderName/Value
        if let Some(extra_map) = extra {
            for (key_str, val_str) in extra_map {
                let name = HeaderName::from_str(&key_str)
                    .map_err(|_| anyhow::anyhow!("Invalid header name: {}", key_str))?;
                let value = HeaderValue::from_str(&val_str)
                    .map_err(|_| anyhow::anyhow!("Invalid header value: {}", val_str))?;
                headers.insert(name, value);
            }
        }

        Ok(headers)
    }

    async fn login_get_cookie(&self) -> anyhow::Result<()> {
        let cf_clearance = self.get_cf_clearance().await?;
        let login_url = format!("https://{}{}", TARGET_URL, "/auth/login");

        // 额外headers
        let mut extra = HashMap::<String, String>::new();
        extra.insert("Referer".to_string(), login_url.clone());
        extra.insert(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded; charset=UTF-8".to_string(),
        );

        let mut headers = self.build_base_request_headers(Some(extra))?;
        headers.insert("Cookie", HeaderValue::from_str(&cf_clearance)?);

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let form_data = [
            ("email", self.email.as_str()),
            ("passwd", self.passwd.as_str()),
            ("code", ""),
        ];
        let res = client.post(login_url).form(&form_data).send().await?;

        if !res.status().is_success() {
            return Err(anyhow::anyhow!("请求错误"));
        }
        let cookies = res.cookies();

        let mut cookie_lines = Vec::with_capacity(4);
        for item in cookies.into_iter() {
            let cookie_line = format!("{}={}", item.name(), item.value());
            cookie_lines.push(cookie_line);
        }
        cookie_lines.push(cf_clearance);

        fs::write(COOKIE_FILE, cookie_lines.join(";")).await?;
        info!("Login successful! Cookie saved to {}", COOKIE_FILE);

        Ok(())
    }

    pub async fn checkin(&self) -> anyhow::Result<()> {
        if !fs::try_exists(COOKIE_FILE).await.unwrap() {
            warn!("{} not exists try flush...", COOKIE_FILE);
            return Err(anyhow::anyhow!("{} not exists", COOKIE_FILE));
        }
        let cookie_str = fs::read_to_string(COOKIE_FILE).await?.trim().to_string();
        if cookie_str.is_empty() {
            return Err(anyhow::anyhow!("cookie.txt is not exists"));
        }
        let mut extra = HashMap::new();
        extra.insert(
            "Referer".to_string(),
            format!("https://{}/user", TARGET_URL),
        );
        extra.insert("TE".to_string(), "trailers".to_string());
        extra.insert("Content-Length".to_string(), "0".to_string());

        let mut headers = self.build_base_request_headers(Some(extra))?;
        headers.insert("Cookie", HeaderValue::from_str(&cookie_str)?); // ? 传播解析错误

        let client = Client::builder().default_headers(headers).build()?;

        let url = format!("https:{}/{}", TARGET_URL, "user/checkin");
        let res = client.post(url).send().await?;
        if !res.status().is_success() {
            return Err(anyhow::anyhow!("请求错误"));
        }
        info!("Status: {}", res.status());

        // 5. 尝试解析 JSON 并 pretty-print
        match res.json::<JsonResult>().await {
            Ok(json_value) => {
                info!("{:?}", json_value);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to parse response as JSON: {}", e);
                Err(anyhow::anyhow!("Cookie expiration"))
            }
        }
    }

    pub async fn flush_cookie(&self) -> anyhow::Result<()> {
        info!("开始登陆刷新Cookie");
        self.login_get_cookie().await?;
        Ok(())
    }
}
