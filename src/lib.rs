mod okti;
pub use okti::*;
use std::fs::File;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init_log_env() {
    let file = File::create("app.log").expect("Failed to create log file");
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")), // 日志级别过滤（从 RUST_LOG 环境变量读取）
        )
        .with(
            fmt::layer()
                .with_file(true) // 启用文件路径
                .with_line_number(true) // 启用行号
                .with_target(true) // 启用 target（模块路径）
                .with_thread_ids(true) // 可选：启用线程 ID
                .with_thread_names(true), // 可选：启用线程名
        )
        .with(
            // 文件层：相同格式，但输出到文件（无线程名以简化）
            fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_target(true)
                .with_thread_ids(true)
                .with_writer(file), // 输出到 app.log 文件
        )
        .init(); // 初始化（全局唯一）
    dotenvy::dotenv().ok(); // .ok() to ignore errors if no .env
}
