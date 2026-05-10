use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RcloneRelease {
    pub version: String,
    pub released: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RcloneUpdateInfo {
    pub current_version: String,
    pub stable: Option<RcloneRelease>,
    pub beta: Option<RcloneRelease>,
    pub stable_update_available: bool,
}

pub fn parse_rclone_update(output: &str) -> Result<RcloneUpdateInfo, String> {
    let mut current_version = None;
    let mut stable = None;
    let mut beta = None;
    let mut reading_beta = false;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("yours:") {
            current_version = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("latest:") {
            stable = parse_release(value.trim());
            reading_beta = false;
        } else if let Some(value) = line.strip_prefix("beta:") {
            reading_beta = true;
            if !value.trim().is_empty() {
                beta = parse_release(value.trim());
            }
        } else if let Some(value) = line.strip_prefix("upgrade:") {
            let url = (!value.trim().is_empty()).then(|| value.trim().to_owned());
            if reading_beta {
                if let Some(release) = &mut beta {
                    release.download_url = url;
                }
            } else if let Some(release) = &mut stable {
                release.download_url = url;
            }
        } else if reading_beta && beta.is_none() {
            beta = parse_release(line);
        }
    }

    let current_version = current_version
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            "rclone returned update information without the installed version.".to_owned()
        })?;
    if let Some(release) = &mut stable {
        release.download_url = Some(format!("https://downloads.rclone.org/v{}", release.version));
    }
    if let Some(release) = &mut beta {
        release.download_url = Some(format!("https://beta.rclone.org/v{}", release.version));
    }
    let stable_update_available = stable.as_ref().is_some_and(|release| {
        compare_versions(&release.version, &current_version) == Ordering::Greater
    });

    Ok(RcloneUpdateInfo {
        current_version,
        stable,
        beta,
        stable_update_available,
    })
}

pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let numeric = |version: &str| {
        version
            .trim_start_matches(['v', 'V'])
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<u64>().unwrap_or_default())
            .collect::<Vec<_>>()
    };
    let left = numeric(left);
    let right = numeric(right);
    for index in 0..left.len().max(right.len()) {
        match left
            .get(index)
            .copied()
            .unwrap_or_default()
            .cmp(&right.get(index).copied().unwrap_or_default())
        {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

fn parse_release(value: &str) -> Option<RcloneRelease> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (version, released) = value
        .strip_suffix(')')
        .and_then(|value| value.rsplit_once(" (released "))
        .map_or((value, None), |(version, released)| {
            (version.trim(), Some(released.trim().to_owned()))
        });
    (!version.is_empty()).then(|| RcloneRelease {
        version: version.to_owned(),
        released,
        download_url: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_and_multiline_beta_channels() {
        let info = parse_rclone_update(
            "yours: 1.74.4\nlatest: 1.75.0 (released 2026-07-31)\n\
             upgrade: https://downloads.rclone.org/v1.75.0\nbeta:\n\
             1.76.0-beta.10147.f0b210a88 (released 2026-08-14)\n\
             upgrade: https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88\n",
        )
        .unwrap();
        assert_eq!(info.current_version, "1.74.4");
        assert_eq!(info.stable.as_ref().unwrap().version, "1.75.0");
        assert_eq!(
            info.beta.as_ref().unwrap().version,
            "1.76.0-beta.10147.f0b210a88"
        );
        assert!(info.stable_update_available);
    }

    #[test]
    fn derives_official_links_when_output_omits_them() {
        let info = parse_rclone_update(
            "yours: 1.75.0\nlatest: 1.75.0 (released 2026-07-31)\n\
             beta: 1.76.0-beta.10147.f0b210a88 (released 2026-08-14)\n",
        )
        .unwrap();
        assert_eq!(
            info.stable.unwrap().download_url.as_deref(),
            Some("https://downloads.rclone.org/v1.75.0")
        );
        assert_eq!(
            info.beta.unwrap().download_url.as_deref(),
            Some("https://beta.rclone.org/v1.76.0-beta.10147.f0b210a88")
        );
        assert!(!info.stable_update_available);
    }

    #[test]
    fn compares_numeric_versions_without_lexical_mistakes() {
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("v1.75.0", "1.75"), Ordering::Equal);
    }
}
