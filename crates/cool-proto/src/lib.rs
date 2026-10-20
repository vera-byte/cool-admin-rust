//! proto 生成代码入口。
//!
//! 由 `build.rs` 通过 tonic-build 编译 `proto/**/*.proto` 得到。

pub mod common {
    pub mod v1 {
        tonic::include_proto!("common.v1");
    }
}

pub mod internal {
    pub mod v1 {
        tonic::include_proto!("internal.v1");
    }
}

pub mod demo {
    pub mod v1 {
        tonic::include_proto!("demo.v1");
    }
}

pub mod base {
    pub mod v1 {
        tonic::include_proto!("base.v1");
    }
}

pub mod dict {
    pub mod v1 {
        tonic::include_proto!("dict.v1");
    }
}

pub mod space {
    pub mod v1 {
        tonic::include_proto!("space.v1");
    }
}

pub mod task {
    pub mod v1 {
        tonic::include_proto!("task.v1");
    }
}

/// 业务成功码（对齐 cool-admin-go）
pub const CODE_SUCCESS: i32 = 1000;
/// 业务失败码
pub const CODE_FAIL: i32 = 1001;

/// gRPC reflection 用文件描述集
pub const REFLECTION_DESCRIPTOR: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/cool_admin_reflection.bin"));
