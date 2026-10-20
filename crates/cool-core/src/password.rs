//! 密码哈希：argon2（新）+ 旧 MD5 识别迁移（决策 D5）。

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand::RngCore;

use crate::error::{CoolError, CoolResult};

/// 生成 argon2 哈希
pub fn hash_password(password: &str) -> CoolResult<String> {
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| CoolError::Internal(format!("salt 生成失败: {e}")))?;

    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CoolError::Internal(format!("密码哈希失败: {e}")))?;

    Ok(hash.to_string())
}

/// 校验密码（argon2）
pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// 旧 MD5（对齐 Go 版 gmd5.Encrypt），仅用于存量数据登录识别
pub fn legacy_md5(password: &str) -> String {
    let digest = md5::compute(password.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// 是否为旧 MD5 哈希（32 位十六进制）
pub fn is_legacy_md5(hash: &str) -> bool {
    hash.len() == 32 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// 校验：优先 argon2，失败后尝试旧 MD5
/// 返回 `(ok, needs_rehash)`，`needs_rehash=true` 表示应升级为 argon2
pub fn verify_any(password: &str, stored: &str) -> (bool, bool) {
    if is_legacy_md5(stored) {
        let ok = legacy_md5(password) == stored.to_lowercase();
        return (ok, ok);
    }
    (verify_password(password, stored), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_roundtrip() {
        let h = hash_password("s3cret!").unwrap();
        assert!(verify_password("s3cret!", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn legacy_md5_match() {
        assert_eq!(legacy_md5("123456"), "e10adc3949ba59abbe56e057f20f883e");
        assert!(is_legacy_md5("e10adc3949ba59abbe56e057f20f883e"));
    }

    #[test]
    fn verify_any_legacy_upgrade() {
        let stored = legacy_md5("123456");
        let (ok, rehash) = verify_any("123456", &stored);
        assert!(ok);
        assert!(rehash);

        let (ok, rehash) = verify_any("bad", &stored);
        assert!(!ok);
        assert!(!rehash);
    }
}
