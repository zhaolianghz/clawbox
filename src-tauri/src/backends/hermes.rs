use super::{Backend, CronJob, GatewayStatus, NewCron};

pub struct HermesBackend;

impl Backend for HermesBackend {
    fn id(&self) -> &'static str { "hermes" }
    fn display_name(&self) -> &'static str { "Hermes" }
    fn version(&self) -> String { "unknown".into() }
    fn is_installed(&self) -> bool { false }
    fn gateway_status(&self) -> Result<GatewayStatus, String> { unimplemented!() }
    fn gateway_start(&self) -> Result<String, String> { unimplemented!() }
    fn gateway_stop(&self) -> Result<String, String> { unimplemented!() }
    fn cron_list(&self) -> Result<Vec<CronJob>, String> { unimplemented!() }
    fn cron_create(&self, _params: NewCron) -> Result<String, String> { unimplemented!() }
    fn cron_remove(&self, _id: &str) -> Result<String, String> { unimplemented!() }
    fn cron_set_enabled(&self, _id: &str, _enabled: bool) -> Result<String, String> { unimplemented!() }
    fn cron_run(&self, _id: &str) -> Result<String, String> { unimplemented!() }
}
