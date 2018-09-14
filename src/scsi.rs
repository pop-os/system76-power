use std::io;
use std::path::PathBuf;
use util::write_file;

const HOSTS: &str = "/sys/class/scsi_host/";

pub trait ScsiPower {
    fn set_power_management_policy(&self, profiles: &[&str]) -> io::Result<()>;
}

pub struct ScsiHosts(Vec<ScsiHost>);

impl ScsiHosts {
    pub fn new() -> ScsiHosts {
        let mut hosts = Vec::new();
        for host in (0..).map(ScsiHost::get) {
            match host {
                Some(host) => hosts.push(host),
                None => break
            }
        }

        ScsiHosts(hosts)
    }
}

impl ScsiPower for ScsiHosts {
    fn set_power_management_policy(&self, profiles: &[&str]) -> io::Result<()> {
        self.0.iter().map(|host| host.set_power_management_policy(profiles)).collect()
    }
}

#[derive(Default, Debug)]
pub struct ScsiHost {
    host: u32,
    link_power_management_policy: PathBuf,
}

impl ScsiHost {
    pub fn get(host: u32) -> Option<ScsiHost> {
        let path = PathBuf::from([HOSTS, "host", &host.to_string(), "/"].concat());
        if !path.exists() {
            return None;
        }

        let link_power_management_policy = path.join("link_power_management_policy");
        if !link_power_management_policy.exists() {
            return None;
        }

        Some(ScsiHost {
            host,
            link_power_management_policy,
        })
    }
}

impl ScsiPower for ScsiHost {
    fn set_power_management_policy(&self, profiles: &[&str]) -> io::Result<()> {
        if profiles.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "at least one scsi power management profile should be specified"
            ));
        }

        let mut last_result = None;

        for prof in profiles {
            debug!("Setting scsi_host {} to {}", self.host, prof);
            last_result = Some(write_file(&self.link_power_management_policy, prof));
            match *last_result.as_ref().unwrap() {
                Ok(_) => break,
                Err(ref why) => error!("Failed to set scsi_host {} to {}: {}", self.host, prof, why)
            }
        }

        last_result.unwrap()
    }
}
