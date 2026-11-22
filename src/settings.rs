//! The mod's page in Options > Mods: which tabs show, and the developer details.

use crate::catalog::Tab;
use mod_api::SettingSpec;

/// One toggle per tab (`tab_mob`, …, on by default), then `dev`.
pub fn settings() -> Vec<SettingSpec> {
    let mut out: Vec<SettingSpec> = Tab::ALL
        .into_iter()
        .map(|tab| {
            SettingSpec::toggle(
                format!("tab_{}", tab.key()),
                format!("jer.setting.tab.{}", tab.key()),
                true,
            )
        })
        .collect();
    out.push(
        SettingSpec::toggle("dev", "jer.setting.dev", false).tooltip("jer.setting.dev.tooltip"),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_is_one_the_game_takes() {
        let all = settings();
        assert_eq!(all.len(), Tab::ALL.len() + 1);
        for setting in all {
            assert_eq!(setting.check(), Ok(()), "{}", setting.key);
        }
    }
}
