use influxdb2::Client;
use rand::{self, distributions::Alphanumeric, Rng};
use std::process::Command as SyncCommand;
use std::time::Duration;
use tokio::{
    net::TcpStream,
    process::Command as AsyncCommand,
    time::{sleep, Instant},
};
use tracing::{info, trace};

#[derive(Debug)]
pub struct TestInflux {
    pub host: String,
    pub org: String,
    pub token: String,
    pub bucket: String,
    pub container_name: String,
    pub client: Client,
}

impl TestInflux {
    /// this panics if it fails to start. thats probably fine for tests, but if not this can easily be changed to return an anyhow::Result
    pub async fn spawn() -> Self {
        let : String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();

        let container_name = format!("docker-test-fixtures-influx-{}", random);

        info!(%container_name);

        let username = "docker_test_fixtures_username";
        let password = "docker_test_fixtures_password";
        let org = "docker_test_fixtures_org";
        let init_bucket = "docker_test_fixtures_bucket";
        let admin_token = "docker_test_fixtures_token";

        let cmd = AsyncCommand::new("docker")
            .args([
                "run",
                "--name",
                &container_name,
                "--rm",
                "-d",
                "-e",
                "DOCKER_INFLUXDB_INIT_MODE=setup",
                "-e",
                &format!("DOCKER_INFLUXDB_INIT_USERNAME={}", username),
                "-e",
                &format!("DOCKER_INFLUXDB_INIT_PASSWORD={}", password),
                "-e",
                &format!("DOCKER_INFLUXDB_INIT_ORG={}", org),
                "-e",
                &format!("DOCKER_INFLUXDB_INIT_BUCKET={}", init_bucket),
                "-e",
                &format!("DOCKER_INFLUXDB_INIT_ADMIN_TOKEN={}", admin_token),
                "-p",
                "0:8086",
                "influxdb:2.6.1-alpine",
            ])
            .output()
            .await
            .expect("failed to start influx");

        // original port 18086
        info!("Creation command is: {:?}", cmd);

        // give the db a second to start
        // TODO: wait until docker says it is healthy
        sleep(Duration::from_secs(1)).await;

        let docker_inspect_output = AsyncCommand::new("docker")
            .args(["inspect", &container_name])
            .output()
            .await
            .unwrap();

        info!(?docker_inspect_output);

        let docker_inspect_json = String::from_utf8(docker_inspect_output.stdout).unwrap();

        info!(%docker_inspect_json);

        let docker_inspect_json: serde_json::Value =
            serde_json::from_str(&docker_inspect_json).unwrap();

        let influx_ports = docker_inspect_json
            .get(0)
            .unwrap()
            .get("NetworkSettings")
            .unwrap()
            .get("Ports")
            .unwrap()
            .get("8086/tcp")
            .unwrap()
            .get(0)
            .unwrap();

        trace!(?influx_ports);

        let influx_port: u64 = influx_ports
            .get("HostPort")
            .expect("unable to determine influx port")
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let influx_ip = influx_ports
            .get("HostIp")
            .and_then(|x| x.as_str())
            .expect("unable to determine influx ip");

        // let host = "http://localhost:8086";
        let influx_host = format!("http://{}:{}", influx_ip, influx_port);
        info!(%influx_host);

        // Create the client ...
        let influxdb_client = influxdb2::Client::new(influx_host.clone(), org, admin_token);
        info!("Influx client is: {:?}", influxdb_client);

        // create the TestInflux as soon as the url is known
        // when this is dropped, the docker container will be stopped
        let test_influx = Self {
            host: influx_host,
            org: org.to_string(),
            token: admin_token.to_string(),
            bucket: init_bucket.to_string(),
            container_name: container_name.clone(),
            client: influxdb_client,
        };

        let start = Instant::now();
        let max_wait = Duration::from_secs(30);
        loop {
            if start.elapsed() > max_wait {
                panic!("influx took too long to start");
            }

            if TcpStream::connect(format!("{}:{}", influx_ip, influx_port))
                .await
                .is_ok()
            {
                break;
            };

            // not open yet. sleep and then try again
            sleep(Duration::from_secs(1)).await;
        }

        sleep(Duration::from_secs(1)).await;

        // TODO: try to use the influx client

        info!(?test_influx, elapsed=%start.elapsed().as_secs_f32(), "influx port is open");

        test_influx
    }
}

impl Drop for TestInflux {
    fn drop(&mut self) {
        info!(%self.container_name, "killing influx");

        // kill -9 is fine since we don't care about the data once the test is done
        let _ = SyncCommand::new("docker")
            .args(["kill", "-s", "9", &self.container_name])
            .output();
    }
}

#[cfg(test)]
mod test {
    use influxdb2::models::Status;

    use super::TestInflux;

    #[test_log::test(tokio::test)]
    async fn start_and_stop() -> anyhow::Result<()> {
        let x = TestInflux::spawn().await;

        let h = x.client.health().await?;

        assert!(h.status == Status::Pass);

        drop(x);

        Ok(())
    }
}
