// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Host prescriptions: the machine a record claims to have run on, as
//! versioned data under `perf/hosts/<id>.yaml`, and the reading of a
//! record's measured fingerprint against it.
//!
//! An absolute millisecond describes a system and the machine it ran on
//! together, so a record names the prescription it claims beside the
//! fingerprint the generator measured, and its derived part says whether the
//! two agree. The prescription is embedded in the record, so the reading is
//! re-derived offline like every other derived number.
//!
//! No openEHR specification governs this: our own design.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::load::{LoadError, compile_schema, load_artifact};
use crate::perf_emulate::fingerprint::EnvironmentFingerprint;

/// The directory under the performance root that holds the prescriptions.
pub const HOSTS_DIR: &str = "hosts";

/// The share a measured memory total may fall below the prescribed one and
/// still match: the kernel and firmware keep part of the installed memory,
/// so an 8 GiB host reports somewhat less.
pub const MEMORY_SLACK_PERCENT: u64 = 10;

/// The kind of machine (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineClass {
    /// A physical machine with no hypervisor between it and the run.
    BareMetal,
    /// A virtual machine.
    VirtualMachine,
    /// A runner a CI service hands out per job: shared hardware no run can
    /// pin, so it never publishes an absolute figure.
    HostedCiRunner,
    /// The source names no class.
    Unstated,
}

/// The storage the database writes to (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageClass {
    /// Local `NVMe`.
    Nvme,
    /// Local SATA or SAS SSD.
    Ssd,
    /// Block storage over the network.
    NetworkBlock,
    /// Spinning disk.
    Hdd,
    /// The source names no storage class.
    Unstated,
}

/// Where the load generator sits relative to the server (a closed
/// vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkLeg {
    /// The generator and the server share one host.
    #[default]
    SameHost,
    /// Different hosts in one zone of one provider.
    SameZone,
    /// Different regions.
    CrossRegion,
}

impl NetworkLeg {
    /// The leg's token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::SameHost => "same-host",
            Self::SameZone => "same-zone",
            Self::CrossRegion => "cross-region",
        }
    }
}

/// The resource ceilings the server's containers run under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerCeilings {
    /// The CPUs the containers may use together.
    pub cpus: f64,
    /// The memory the containers may use together, in GiB.
    pub memory_gib: u32,
}

/// One host prescription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPrescription {
    /// The prescription's id, its file stem.
    pub id: String,
    /// An integer version, raised whenever a field changes.
    pub version: u32,
    /// What the host is, in a sentence.
    pub description: String,
    /// Where the prescription comes from.
    pub derivation: String,
    /// The kind of machine.
    pub machine_class: MachineClass,
    /// The CPUs the host offers.
    pub vcpu: u32,
    /// The memory the host offers, in GiB.
    pub memory_gib: u32,
    /// The storage the database writes to.
    pub storage_class: StorageClass,
    /// The container ceilings, absent when the containers run unconstrained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_ceilings: Option<ContainerCeilings>,
    /// Where the generator sits relative to the server.
    pub network_leg: NetworkLeg,
    /// Whether the host runs nothing but the measurement.
    pub idle: bool,
}

static HOST_SCHEMA: LazyLock<Result<jsonschema::Validator, String>> = LazyLock::new(|| {
    compile_schema(&crate::schema::perf_host_schema(), "perf-host.schema.json")
        .map_err(|e| e.to_string())
});

/// Loads the prescription at `path` through its schema and its gates: the
/// id is the file stem, the version and the counts are positive, and the
/// container ceilings fit inside the host.
///
/// # Errors
/// [`LoadError`] naming the file and the field.
pub fn load_host(path: &Path) -> Result<HostPrescription, LoadError> {
    let validator = HOST_SCHEMA.as_ref().map_err(|message| LoadError::Schema {
        path: PathBuf::from("perf-host.schema.json"),
        message: message.clone(),
    })?;
    let host: HostPrescription = load_artifact(path, validator)?;
    let model = |message: String| LoadError::Model {
        path: path.to_owned(),
        message,
    };
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if host.id != stem {
        return Err(model(format!(
            "id: `{}` is not the file stem `{stem}`",
            host.id
        )));
    }
    if host.version == 0 || host.vcpu == 0 || host.memory_gib == 0 {
        return Err(model(
            "version, vcpu and memory_gib: each must be at least 1".to_owned(),
        ));
    }
    if let Some(ceilings) = &host.container_ceilings {
        if !(ceilings.cpus > 0.0 && ceilings.cpus <= f64::from(host.vcpu)) {
            return Err(model(format!(
                "container_ceilings.cpus: {} is not inside the host's {} vCPU",
                ceilings.cpus, host.vcpu
            )));
        }
        if ceilings.memory_gib == 0 || ceilings.memory_gib > host.memory_gib {
            return Err(model(format!(
                "container_ceilings.memory_gib: {} is not inside the host's {} GiB",
                ceilings.memory_gib, host.memory_gib
            )));
        }
    }
    Ok(host)
}

/// Loads the prescription `id` from `<perf_root>/hosts/<id>.yaml`.
///
/// # Errors
/// [`LoadError`] when the file is missing or refused.
pub fn load_host_id(perf_root: &Path, id: &str) -> Result<HostPrescription, LoadError> {
    load_host(&perf_root.join(HOSTS_DIR).join(format!("{id}.yaml")))
}

/// Loads every prescription under `<perf_root>/hosts/`, in file-name order.
///
/// # Errors
/// [`LoadError`] for the first file that does not load.
pub fn load_hosts(perf_root: &Path) -> Result<Vec<HostPrescription>, LoadError> {
    let dir = perf_root.join(HOSTS_DIR);
    let entries = std::fs::read_dir(&dir).map_err(|source| LoadError::Io {
        path: dir.clone(),
        source,
    })?;
    let mut paths = entries
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| LoadError::Io {
            path: dir.clone(),
            source,
        })?;
    paths.retain(|p| p.extension().is_some_and(|x| x == "yaml"));
    paths.sort();
    paths.iter().map(|p| load_host(p)).collect()
}

/// How a record's measured host reads against the prescription it claims
/// (a closed vocabulary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostReading {
    /// The record claims no prescription.
    Unclaimed,
    /// Every field the fingerprint discloses agrees with the prescription.
    Matched {
        /// The prescription's id.
        prescription: String,
    },
    /// A disclosed field contradicts the prescription: the record ran on
    /// another machine than the one it claims.
    Unmatched {
        /// The prescription's id.
        prescription: String,
        /// Each contradiction, as a sentence.
        mismatches: Vec<String>,
    },
    /// The fingerprint does not disclose what the prescription needs.
    Unverifiable {
        /// The prescription's id.
        prescription: String,
        /// The fingerprint fields that are absent.
        missing: Vec<String>,
    },
}

/// Reads `fingerprint` and `leg` against `prescription`.
///
/// The CPU count must equal the prescribed one, the memory must lie within
/// [`MEMORY_SLACK_PERCENT`] below the prescribed GiB and never above it, and
/// the network leg must be the prescribed one.
#[must_use]
pub fn read_host(
    prescription: Option<&HostPrescription>,
    fingerprint: &EnvironmentFingerprint,
    leg: NetworkLeg,
) -> HostReading {
    let Some(host) = prescription else {
        return HostReading::Unclaimed;
    };
    let mut mismatches = Vec::new();
    let mut missing = Vec::new();
    match fingerprint.available_parallelism {
        Some(cpus) if cpus != host.vcpu => mismatches.push(format!(
            "the generator measured {cpus} CPUs and the prescription names {}",
            host.vcpu
        )),
        Some(_) => {}
        None => missing.push("available_parallelism".to_owned()),
    }
    match fingerprint.total_memory_bytes {
        Some(bytes) => {
            let prescribed = u64::from(host.memory_gib).saturating_mul(1 << 30);
            let floor = prescribed
                .saturating_mul(100 - MEMORY_SLACK_PERCENT)
                .div_euclid(100);
            if bytes > prescribed || bytes < floor {
                mismatches.push(format!(
                    "the generator measured {bytes} bytes of memory and the prescription names {} GiB",
                    host.memory_gib
                ));
            }
        }
        None => missing.push("total_memory_bytes".to_owned()),
    }
    if leg != host.network_leg {
        mismatches.push(format!(
            "the record names the {} network leg and the prescription {}",
            leg.token(),
            host.network_leg.token()
        ));
    }
    let prescription = host.id.clone();
    if !mismatches.is_empty() {
        HostReading::Unmatched {
            prescription,
            mismatches,
        }
    } else if !missing.is_empty() {
        HostReading::Unverifiable {
            prescription,
            missing,
        }
    } else {
        HostReading::Matched { prescription }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> HostPrescription {
        HostPrescription {
            id: "reference".to_owned(),
            version: 1,
            description: "d".to_owned(),
            derivation: "d".to_owned(),
            machine_class: MachineClass::Unstated,
            vcpu: 8,
            memory_gib: 8,
            storage_class: StorageClass::Unstated,
            container_ceilings: None,
            network_leg: NetworkLeg::SameHost,
            idle: true,
        }
    }

    fn fingerprint(cpus: Option<u32>, bytes: Option<u64>) -> EnvironmentFingerprint {
        EnvironmentFingerprint {
            arch: "x86_64".to_owned(),
            os: "linux".to_owned(),
            available_parallelism: cpus,
            cpu_model: None,
            total_memory_bytes: bytes,
        }
    }

    #[test]
    fn a_host_inside_the_prescription_matches() {
        let reading = read_host(
            Some(&reference()),
            &fingerprint(Some(8), Some(8_000_000_000)),
            NetworkLeg::SameHost,
        );
        assert_eq!(
            reading,
            HostReading::Matched {
                prescription: "reference".to_owned()
            }
        );
    }

    #[test]
    fn another_machine_is_labelled_unmatched_with_each_contradiction() {
        let reading = read_host(
            Some(&reference()),
            &fingerprint(Some(4), Some(16 << 30)),
            NetworkLeg::SameZone,
        );
        let HostReading::Unmatched { mismatches, .. } = reading else {
            panic!("expected unmatched, got {reading:?}");
        };
        assert_eq!(mismatches.len(), 3, "{mismatches:?}");
    }

    #[test]
    fn a_fingerprint_without_the_fields_is_unverifiable() {
        let reading = read_host(
            Some(&reference()),
            &fingerprint(None, None),
            NetworkLeg::SameHost,
        );
        assert!(
            matches!(reading, HostReading::Unverifiable { ref missing, .. } if missing.len() == 2),
            "{reading:?}"
        );
    }

    #[test]
    fn no_claim_reads_unclaimed() {
        assert_eq!(
            read_host(None, &fingerprint(Some(8), None), NetworkLeg::SameHost),
            HostReading::Unclaimed
        );
    }
}
