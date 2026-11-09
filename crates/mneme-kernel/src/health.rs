use serde::{Deserialize, Serialize};

/// Kernel 对外暴露的最小健康状态。
/// 后续可以继续增加 ledger 状态、worker 状态、数据库状态等字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelHealth {
    pub service: String,
    pub status: String,
    pub version: String,
}

pub fn kernel_health() -> KernelHealth {
    KernelHealth {
        service: "mneme-kernel".to_string(),
        // 对外健康检查返回 ready，表示 Kernel 基础模块已经可以响应。
        status: "ready".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_health_reports_ready_service_identity() {
        let health = kernel_health();

        assert_eq!(health.service, "mneme-kernel");
        assert_eq!(health.status, "ready");
        assert_eq!(health.version, "0.1.0");
    }
}
