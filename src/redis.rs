use redis::Client;
use std::time::Duration;
use tokio::{
    net::TcpStream,
    process::Command as AsyncCommand,
    time::{sleep, Instant},
};
use tracing::{info, trace};

use crate::docker::{random_container_name, TestContainer};

#[derive(Debug)]
pub struct TestRedis {
    pub url: String,
    pub container_name: String,
    pub client: Client,
}

impl TestRedis {
    /// this panics if it fails to start. thats probably fine for tests, but if not this can easily be changed to return an anyhow::Result
    pub async fn spawn(docker_tag: &str) -> Self {
        let container_name = random_container_name("redis");

        info!(%container_name);

        let cmd = AsyncCommand::new("docker")
            .args([
                "run",
                "--name",
                &container_name,
                "--rm",
                "-d",
                "-p",
                "0:6379",
                &format!("redis:{docker_tag}"),
            ])
            .output()
            .await
            .expect("failed to start influx");

        info!("Creation command is: {:?}", cmd);

        // give the container a second to start
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

        let container_ports = docker_inspect_json
            .get(0)
            .unwrap()
            .get("NetworkSettings")
            .unwrap()
            .get("Ports")
            .unwrap()
            .get("6379/tcp")
            .unwrap()
            .get(0)
            .unwrap();

        trace!(?container_ports);

        let container_port: u64 = container_ports
            .get("HostPort")
            .expect("unable to determine influx port")
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let container_ip = container_ports
            .get("HostIp")
            .and_then(|x| x.as_str())
            .expect("unable to determine influx ip");

        let container_url = format!("redis://{}:{}", container_ip, container_port);
        info!(%container_url);

        // Create the client ...
        let client = Client::open(container_url.clone()).unwrap();
        info!(?client);

        // create the TestInflux as soon as the url is known
        // when this is dropped, the docker container will be stopped
        let x = Self {
            url: container_url,
            container_name: container_name.clone(),
            client,
        };

        let start = Instant::now();
        let max_wait = Duration::from_secs(30);
        loop {
            if start.elapsed() > max_wait {
                panic!("influx took too long to start");
            }

            if TcpStream::connect(format!("{}:{}", container_ip, container_port))
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

        info!(?x, elapsed=%start.elapsed().as_secs_f32(), "port is open");

        x
    }
}

impl TestContainer for TestRedis {
    fn container_name(&self) -> &str {
        self.container_name.as_str()
    }
}

impl Drop for TestRedis {
    fn drop(&mut self) {
        self.kill_container()
    }
}

#[cfg(test)]
mod test {
    use super::TestRedis;

    #[test_log::test(tokio::test)]
    async fn start_and_stop() -> anyhow::Result<()> {
        let x = TestRedis::spawn("7-alpine").await;

        let mut con = x.client.get_connection()?;

        redis::cmd("SET")
            .arg("my_key")
            .arg(42)
            .query::<()>(&mut con)?;

        Ok(())
    }
}
