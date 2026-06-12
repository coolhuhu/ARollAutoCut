use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub bundle_identifier: &'static str,
}

impl AppInfo {
    pub const fn current() -> Self {
        Self {
            name: "ARollCut",
            version: env!("CARGO_PKG_VERSION"),
            bundle_identifier: "com.coolhuhu.arollcut",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppInfo;

    #[test]
    fn exposes_confirmed_application_identity() {
        let info = AppInfo::current();

        assert_eq!(info.name, "ARollCut");
        assert_eq!(info.version, "0.1.0");
        assert_eq!(info.bundle_identifier, "com.coolhuhu.arollcut");
    }
}
