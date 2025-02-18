use super::TStep;
use crate::executor::values_validator_path;
use crate::types::interpretable::Interpretable;
use crate::types::mixed_fs::MixedFS;
use crate::types::permissions::{Generalizable, Permission, PermissionLevel};
use crate::types::workflow::WorkflowContext;
use crate::utils::env::{env_desktop, env_start_menu};
use crate::utils::fs::{count_sub_files, try_recycle};
use crate::{
    log, p2s, types::verifiable::Verifiable, utils::path::parse_relative_path_with_located,
};
use anyhow::{anyhow, Result};
use mslnk::ShellLink;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::create_dir_all;
use std::path::Path;
use std::ptr::null_mut;
use winapi::shared::minwindef::{LPARAM, WPARAM};
use winapi::um::winuser::{
    SendMessageTimeoutA, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

lazy_static! {
    static ref TARGET_RE: Regex = Regex::new(r"^(([^/]+)/)?([^/]+)$").unwrap();
}

// 返回的第二参数表示是否创建了父目录
fn parse_target(name: &String, base: &String) -> Result<(String, bool)> {
    // 匹配 target_name 模式
    let sp: Vec<&str> = name.split("/").collect();
    let length = sp.len();
    let (lnk_folder_opt, lnk_name) = if length > 2 {
        return Err(anyhow!(
            "Error(Link):Invalid field 'target_name', expect 'NAME' or 'FOLDER/NAME', got '{name}'"
        ));
    } else if length == 2 {
        (
            Some(sp.get(0).unwrap().to_string()),
            sp.get(1).unwrap().to_string(),
        )
    } else {
        (None, sp.get(0).unwrap().to_string())
    };

    // 解析目标位置
    let target = if let Some(lnk_folder) = lnk_folder_opt {
        let dir = Path::new(base).join(&lnk_folder);
        if !dir.exists() {
            create_dir_all(dir).map_err(|e| {
                anyhow!("Error(Link):Failed to create directory '{base}/{lnk_folder}' : {e}")
            })?;
        }
        (format!("{base}/{lnk_folder}/{lnk_name}.lnk"), true)
    } else {
        (format!("{base}/{lnk_name}.lnk"), false)
    };

    Ok(target)
}

fn create_shortcut(sl: &ShellLink, name: &String, base: &String) -> Result<()> {
    let (target, _) = parse_target(name, base)?;
    sl.create_lnk(&target)
        .map_err(|err| anyhow!("Error(Link):Can't create shortcut {target} : {err}"))?;
    log!("Info(Link):Added shortcut '{target}'");
    Ok(())
}

fn delete_shortcut(name: &String, base: &String) -> Result<()> {
    let (target, parent) = parse_target(name, base)?;
    try_recycle(&target)?;
    if parent {
        let parent_path = Path::new(&target).parent().unwrap();
        if count_sub_files(parent_path, |name| {
            name.ends_with(".lnk") || name.ends_with(".LNK")
        })? == 0
        {
            if let Err(e) = try_recycle(parent_path) {
                log!(
                    "Warning(Link):Failed to delete empty shortcut directory '{p}' : {e}",
                    p = p2s!(parent_path)
                );
            }
        }
    }
    log!("Info(Link):Removed shortcut '{target}'");
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StepLink {
    pub source_file: String,
    pub target_name: Option<String>,
    pub target_args: Option<String>,
    pub target_icon: Option<String>,
    pub at: Option<Vec<String>>,
}

impl StepLink {
    fn get_target_name(&self) -> String {
        self.target_name.to_owned().unwrap_or_else(|| {
            let p = Path::new(&self.source_file);
            p2s!(p.file_stem().unwrap())
        })
    }
}

impl TStep for StepLink {
    fn run(self, cx: &mut WorkflowContext) -> anyhow::Result<i32> {
        // 确定 target_name
        let target_name = self.get_target_name();

        // 解析源文件绝对路径
        let abs_clear_source_path =
            parse_relative_path_with_located(&self.source_file, &cx.located);
        // println!("{abs_clear_source_path:?}");
        let abs_clear_source = p2s!(abs_clear_source_path);

        // 创建实例
        let mut sl = ShellLink::new(&abs_clear_source)
            .map_err(|_| anyhow!("Error(Link):Can't find source file '{abs_clear_source}'"))?;

        // 填充额外参数
        if self.target_icon.is_some() {
            sl.set_icon_location(self.target_icon.map(|relative_icon| {
                p2s!(parse_relative_path_with_located(
                    &relative_icon,
                    &cx.located
                ))
            }));
        }
        if self.target_args.is_some() {
            sl.set_arguments(self.target_args);
        }

        // 分流
        let set: HashSet<String> =
            HashSet::from_iter(self.at.clone().unwrap_or(vec!["Desktop".to_string()]));
        if set.contains("Desktop") {
            create_shortcut(&sl, &target_name, &env_desktop())?;
        }
        if set.contains("StartMenu") {
            create_shortcut(&sl, &target_name, &env_start_menu())?;
            // 发送全局广播
            let result = unsafe {
                SendMessageTimeoutA(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0 as WPARAM,
                    0 as LPARAM,
                    SMTO_ABORTIFHUNG,
                    3000,
                    null_mut(),
                )
            };
            if result == 0 {
                log!("Warning(Link):Failed to apply start menu change, restart is required")
            }
        }

        Ok(0)
    }
    fn reverse_run(self, _: &mut WorkflowContext) -> Result<()> {
        let set: HashSet<String> =
            HashSet::from_iter(self.at.clone().unwrap_or(vec!["Desktop".to_string()]));
        let target_name = self.get_target_name();
        if set.contains("Desktop") {
            delete_shortcut(&target_name, &env_desktop())?;
        }
        if set.contains("StartMenu") {
            delete_shortcut(&target_name, &env_start_menu())?;
            // 发送全局广播
            let result = unsafe {
                SendMessageTimeoutA(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0 as WPARAM,
                    0 as LPARAM,
                    SMTO_ABORTIFHUNG,
                    3000,
                    null_mut(),
                )
            };
            if result == 0 {
                log!("Warning(Link):Failed to apply start menu change, restart is required")
            }
        }
        Ok(())
    }
    fn get_manifest(&self, _fs: &mut MixedFS) -> Vec<String> {
        vec![self.source_file.to_owned()]
    }
}

impl Interpretable for StepLink {
    fn interpret<F>(self, interpreter: F) -> Self
    where
        F: Fn(String) -> String,
    {
        Self {
            source_file: interpreter(self.source_file),
            target_name: self.target_name.map(&interpreter),
            target_args: self.target_args.map(&interpreter),
            target_icon: self.target_icon.map(&interpreter),
            at: self.at,
        }
    }
}

impl Verifiable for StepLink {
    fn verify_self(&self, _: &String) -> Result<()> {
        values_validator_path(&self.source_file)?;
        if let Some(target_name) = &self.target_name {
            if !TARGET_RE.is_match(&target_name) {
                return Err(anyhow!(
                    "Error(Link):Invalid field 'target_name', expect 'NAME' or 'FOLDER/NAME', got '{target_name}'"
                ));
            }
            if target_name.contains("..") {
                return Err(anyhow!(
                    "Error(Link):Invalid field 'target_name' : shouldn't contain '..', got '{target_name}'"
                ));
            }
            if target_name.to_lowercase().ends_with(".lnk") {
                return Err(anyhow!(
                    "Error(Link):Invalid field 'target_name' : shouldn't end with '.lnk', got '{target_name}'"
                ));
            }
        }

        Ok(())
    }
}

impl Generalizable for StepLink {
    fn generalize_permissions(&self) -> Result<Vec<Permission>> {
        let mut keys = Vec::new();
        if let Some(ats) = &self.at {
            if ats.contains(&"Desktop".to_string()) {
                keys.push("link_desktop")
            }
            if ats.contains(&"StartMenu".to_string()) {
                keys.push("link_startmenu")
            }
        } else {
            keys.push("link_desktop")
        }

        let res: Vec<Permission> = keys
            .into_iter()
            .map(|key| Permission {
                key: key.to_string(),
                level: PermissionLevel::Normal,
                targets: vec![self.get_target_name()],
            })
            .collect();

        Ok(res)
    }
}

#[test]
fn test_link() {
    use std::fs::{remove_dir, remove_file};
    let mut cx = WorkflowContext::_demo();

    // 配置拉满
    let step = StepLink {
        source_file: String::from("examples/VSCode/VSCode/Code.exe"),
        target_name: Some(String::from("ms_ept_test/VSC")),
        target_args: Some("--debug".to_string()),
        target_icon: Some("examples/VSCode/VSCode/favicon.ico".to_string()),
        at: Some(vec!["Desktop".to_string(), "StartMenu".to_string()]),
    };
    step.verify_self(&String::from("./examples/VSCode/VSCode"))
        .unwrap();
    step.run(&mut cx).unwrap();

    let desktop_path = dirs::desktop_dir().unwrap().join("ms_ept_test/VSC.lnk");
    let desktop_folder_path = dirs::desktop_dir().unwrap().join("ms_ept_test");
    let start_path = Path::new(&env_start_menu()).join("ms_ept_test/VSC.lnk");

    assert!(desktop_path.exists());
    assert!(start_path.exists());

    remove_file(desktop_path).unwrap();
    remove_dir(desktop_folder_path).unwrap();
    remove_file(start_path).unwrap();

    // 缺省状态
    StepLink {
        source_file: String::from("examples/VSCode/VSCode/Code.exe"),
        target_name: None,
        target_args: None,
        target_icon: None,
        at: None,
    }
    .run(&mut cx)
    .unwrap();
    let desktop_path = dirs::desktop_dir().unwrap().join("Code.lnk");
    assert!(desktop_path.exists());
    remove_file(desktop_path).unwrap();

    // 重命名
    StepLink {
        source_file: String::from("examples/VSCode/VSCode/Code.exe"),
        target_name: Some("vsc".to_string()),
        target_args: None,
        target_icon: None,
        at: None,
    }
    .run(&mut cx)
    .unwrap();
    let desktop_path = dirs::desktop_dir().unwrap().join("vsc.lnk");
    assert!(desktop_path.exists());
    remove_file(desktop_path).unwrap();
}
