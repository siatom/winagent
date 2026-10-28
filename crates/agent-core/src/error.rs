use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),
    #[error("模型服务返回 {status}: {message}")]
    Api { status: u16, message: String },
    #[error("响应解析失败: {0}")]
    Parse(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("模型没有返回任何内容")]
    Empty,
    #[error("已取消")]
    Cancelled,
}
