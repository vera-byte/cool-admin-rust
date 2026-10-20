//! EPS 元数据注册（对齐 cool-admin-go base_eps_admin）。
//!
//! gRPC 版由各模块在启动时注册服务/RPC 元数据，
//! `BaseOpenService.Eps` / `EpsApp` 查询返回。

use std::collections::HashMap;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use serde_json::{Value, json};

/// 单个 API 元数据
#[derive(Debug, Clone)]
pub struct ApiMeta {
    /// 所属模块，如 base / dict
    pub module: String,
    /// RPC 方法名，如 Page
    pub method: String,
    /// 调用路径，如 base.v1.BaseSysUserService/Page
    pub path: String,
    /// 路由前缀，如 base.v1.BaseSysUser
    pub prefix: String,
    /// 描述
    pub summary: String,
}

/// 模块元数据
#[derive(Debug, Clone, Default)]
pub struct ModuleMeta {
    pub module: String,
    pub prefix: String,
    pub api: Vec<ApiMeta>,
}

static REGISTRY: Lazy<RwLock<HashMap<String, ModuleMeta>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 注册一个 API
pub fn register_api(meta: ApiMeta) {
    let mut map = REGISTRY.write().unwrap();
    let entry = map
        .entry(meta.module.clone())
        .or_insert_with(|| ModuleMeta {
            module: meta.module.clone(),
            prefix: meta.prefix.clone(),
            api: vec![],
        });
    entry.api.push(meta);
}

/// 批量注册（模块级）
pub fn register_module(module: &str, prefix: &str, apis: &[(&str, &str, &str)]) {
    for (method, path, summary) in apis {
        register_api(ApiMeta {
            module: module.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            prefix: prefix.to_string(),
            summary: summary.to_string(),
        });
    }
}

/// 导出 admin EPS（map<module, ModuleMeta JSON>）
pub fn admin_eps() -> Value {
    let map = REGISTRY.read().unwrap();
    let mut out = serde_json::Map::new();
    for (name, m) in map.iter() {
        out.insert(
            name.clone(),
            json!({
                "module": m.module,
                "prefix": m.prefix,
                "api": m.api.iter().map(|a| json!({
                    "module": a.module,
                    "method": a.method,
                    "path": a.path,
                    "prefix": a.prefix,
                    "summary": a.summary,
                })).collect::<Vec<_>>(),
                "columns": [],
            }),
        );
    }
    Value::Object(out)
}

/// 导出 app EPS（精简：仅 path/method）
pub fn app_eps() -> Value {
    let map = REGISTRY.read().unwrap();
    let mut list = Vec::new();
    for m in map.values() {
        for a in &m.api {
            list.push(json!({
                "module": a.module,
                "method": a.method,
                "path": a.path,
                "summary": a.summary,
            }));
        }
    }
    json!(list)
}

/// 清空（测试用）
#[cfg(test)]
pub fn clear() {
    REGISTRY.write().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_export() {
        clear();
        register_module(
            "base",
            "base.v1.BaseSysUser",
            &[
                ("Page", "base.v1.BaseSysUserService/Page", "用户分页"),
                ("Info", "base.v1.BaseSysUserService/Info", "用户详情"),
            ],
        );
        let v = admin_eps();
        assert!(v.get("base").is_some());
        assert_eq!(v["base"]["api"].as_array().unwrap().len(), 2);
        let app = app_eps();
        assert_eq!(app.as_array().unwrap().len(), 2);
        clear();
    }
}
