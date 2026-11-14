//! Garde-fou thermique (`[thermal]`) : au-dessus de `pause_c`, aucune nouvelle partie ; elles
//! reprennent sous `resume_c`. Les parties en cours finissent. Un CPU trop chaud baisse sa
//! fréquence : les jars mettent alors plus que `connect_timeout_ms` à démarrer, et perdent sur
//! un `no_show` dont le serveur est responsable.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::ThermalConfig;

/// Pilotes hwmon dont `temp1_input` est la température du package (Intel, AMD, puce ARM).
const PACKAGE_HWMON: [&str; 6] = [
    "coretemp",
    "k10temp",
    "zenpower",
    "cpu_thermal",
    "soc_thermal",
    "soc-thermal",
];

pub struct Thermal {
    sensor: PathBuf,
    pause_c: f64,
    resume_c: f64,
    hot: bool,
    unreadable: bool,
}

impl Thermal {
    /// `None` sans `[thermal]`, ou sans capteur (journalisé).
    pub fn new(cfg: Option<&ThermalConfig>) -> Option<Self> {
        let cfg = cfg?;
        let Some(sensor) = cfg.sensor.clone().or_else(|| find_sensor(Path::new("/sys/class"))) else {
            eprintln!("[thermal] aucun capteur de température trouvé : garde-fou désactivé");
            return None;
        };
        eprintln!(
            "[thermal] capteur {} : pause au-dessus de {} °C, reprise sous {} °C",
            sensor.display(),
            cfg.pause_c,
            cfg.resume_c
        );
        Some(Self {
            sensor,
            pause_c: cfg.pause_c,
            resume_c: cfg.resume_c,
            hot: false,
            unreadable: false,
        })
    }

    /// Faux tant que le CPU est trop chaud pour lancer une partie. Un capteur illisible ne bloque
    /// rien : mieux vaut jouer sans garde-fou que plus du tout.
    pub fn cool(&mut self) -> bool {
        match read_celsius(&self.sensor) {
            Ok(celsius) => {
                self.unreadable = false;
                let hot = step(self.hot, celsius, self.pause_c, self.resume_c);
                if hot != self.hot {
                    if hot {
                        eprintln!(
                            "[thermal] CPU à {celsius:.0} °C : plus de nouvelle partie jusqu'à {} °C",
                            self.resume_c
                        );
                    } else {
                        eprintln!("[thermal] CPU à {celsius:.0} °C : reprise des parties");
                    }
                }
                self.hot = hot;
            }
            Err(e) => {
                if !self.unreadable {
                    eprintln!(
                        "[thermal] {} illisible ({e}) : parties lancées sans garde-fou",
                        self.sensor.display()
                    );
                }
                self.unreadable = true;
                self.hot = false;
            }
        }
        !self.hot
    }
}

/// Hystérésis : chaud au-dessus de `pause_c`, de nouveau froid seulement sous `resume_c`.
fn step(hot: bool, celsius: f64, pause_c: f64, resume_c: f64) -> bool {
    if hot { celsius >= resume_c } else { celsius >= pause_c }
}

/// sysfs donne des millidegrés.
fn read_celsius(path: &Path) -> io::Result<f64> {
    let text = fs::read_to_string(path)?;
    let milli: f64 = text
        .trim()
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{text:?} : {e}")))?;
    Ok(milli / 1000.0)
}

/// Température du package sous `class` (`/sys/class`) : hwmon d'abord, sinon la zone `x86_pkg_temp`.
fn find_sensor(class: &Path) -> Option<PathBuf> {
    let entries = |dir: &str| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = fs::read_dir(class.join(dir))
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        paths.sort();
        paths
    };
    let named = |dir: &Path, file: &str, names: &[&str]| {
        fs::read_to_string(dir.join(file)).is_ok_and(|n| names.contains(&n.trim()))
    };
    let hwmon = entries("hwmon")
        .into_iter()
        .find(|d| named(d, "name", &PACKAGE_HWMON))
        .map(|d| d.join("temp1_input"));
    hwmon.filter(|p| p.is_file()).or_else(|| {
        entries("thermal")
            .into_iter()
            .find(|d| named(d, "type", &["x86_pkg_temp", "soc-thermal"]))
            .map(|d| d.join("temp"))
            .filter(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::submit::test_support::temp_dir;

    #[test]
    fn the_pause_lasts_until_the_resume_threshold() {
        let mut hot = false;
        let states: Vec<bool> = [70.0, 89.9, 90.0, 85.0, 80.0, 79.9, 89.0, 95.0]
            .into_iter()
            .map(|c| {
                hot = step(hot, c, 90.0, 80.0);
                hot
            })
            .collect();
        assert_eq!(states, [false, false, true, true, true, false, false, true]);
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_package_sensor_is_found_in_hwmon_then_in_thermal_zones() {
        let dir = temp_dir("thermal-find");
        let class = dir.as_path();
        assert_eq!(find_sensor(class), None);
        write(&class.join("thermal/thermal_zone0/type"), "acpitz\n");
        write(&class.join("thermal/thermal_zone0/temp"), "40000\n");
        write(&class.join("thermal/thermal_zone1/type"), "x86_pkg_temp\n");
        write(&class.join("thermal/thermal_zone1/temp"), "71000\n");
        let zone = class.join("thermal/thermal_zone1/temp");
        assert_eq!(find_sensor(class), Some(zone.clone()));
        assert!((read_celsius(&zone).unwrap() - 71.0).abs() < f64::EPSILON);
        write(&class.join("hwmon/hwmon0/name"), "nvme\n");
        write(&class.join("hwmon/hwmon0/temp1_input"), "30000\n");
        write(&class.join("hwmon/hwmon3/name"), "coretemp\n");
        write(&class.join("hwmon/hwmon3/temp1_input"), "88000\n");
        assert_eq!(find_sensor(class), Some(class.join("hwmon/hwmon3/temp1_input")));
    }

    #[test]
    fn the_soc_sensor_of_an_arm_board_is_found() {
        let dir = temp_dir("thermal-arm");
        let class = dir.as_path();
        write(&class.join("thermal/thermal_zone0/type"), "soc-thermal\n");
        write(&class.join("thermal/thermal_zone0/temp"), "51769\n");
        assert_eq!(find_sensor(class), Some(class.join("thermal/thermal_zone0/temp")));
        write(&class.join("hwmon/hwmon0/name"), "soc_thermal\n");
        write(&class.join("hwmon/hwmon0/temp1_input"), "51769\n");
        write(&class.join("hwmon/hwmon1/name"), "bigcore0_thermal\n");
        write(&class.join("hwmon/hwmon1/temp1_input"), "52000\n");
        assert_eq!(find_sensor(class), Some(class.join("hwmon/hwmon0/temp1_input")));
    }

    #[test]
    fn an_unreadable_sensor_never_blocks_the_games() {
        let sensor = temp_dir("thermal-read").join("temp");
        let cfg = ThermalConfig {
            sensor: Some(sensor.clone()),
            ..ThermalConfig::default()
        };
        let mut thermal = Thermal::new(Some(&cfg)).unwrap();
        assert!(thermal.cool());
        fs::write(&sensor, "95000\n").unwrap();
        assert!(!thermal.cool());
        fs::write(&sensor, "85000\n").unwrap();
        assert!(!thermal.cool());
        fs::write(&sensor, "pas un nombre\n").unwrap();
        assert!(thermal.cool());
        fs::write(&sensor, "79000\n").unwrap();
        assert!(thermal.cool());
    }
}
