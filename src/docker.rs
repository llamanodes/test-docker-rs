use rand::{self, distributions::Alphanumeric, Rng};
use std::process::Command as SyncCommand;
use tracing::info;

/// TODO: we need a 0% chance of collisions
pub fn random_container_name(service: &str) -> String {
    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    format!("docker-test-fixtures-{}-{}", service, random)
}

pub trait TestContainer {
    fn container_name(&self) -> &str;

    /// TODO: proc macro to be sure this is handled in a Drop
    fn kill_container(&mut self) {
        let container = self.container_name();

        info!(%container, "killing");

        // kill -9 is fine since we don't care about the data once the test is done
        let _ = SyncCommand::new("docker")
            .args(["kill", "-s", "9", container])
            .output();
    }
}
