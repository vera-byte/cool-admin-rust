//! 响应构造助手（对齐 cool-admin-go `Ok` / `Fail`）。
//!
//! 决策 D1：各 RPC 使用 typed data，此处只提供 code/message 约定与通用填充函数。

pub use cool_proto::{CODE_FAIL, CODE_SUCCESS};

/// 成功消息占位；带 i18n 后走 `cool_core::i18n`。
pub const DEFAULT_OK_MESSAGE: &str = "ok";

/// 填充成功响应头
pub fn ok_code() -> (i32, String) {
    (CODE_SUCCESS, DEFAULT_OK_MESSAGE.to_string())
}

/// 填充失败响应头
pub fn fail_code(message: impl Into<String>) -> (i32, String) {
    (CODE_FAIL, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_and_fail_codes() {
        let (c, m) = ok_code();
        assert_eq!(c, 1000);
        assert_eq!(m, "ok");

        let (c, m) = fail_code("boom");
        assert_eq!(c, 1001);
        assert_eq!(m, "boom");
    }
}
