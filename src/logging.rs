//! 统一日志输出: 所有打印走本模块宏, 自动带本地时间前缀.
//! 容器需设置 TZ=Asia/Shanghai(镜像已含 zoneinfo)才能得到正确的本地时间.

/// 带时间戳的普通日志: `[2026-11-18 08:30:12] 消息`
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*))
    };
}

/// 带时间戳的错误日志: `[2026-11-18 08:30:12] [ERROR] 消息`, 走 stderr
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!("[{}] [ERROR] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*))
    };
}
