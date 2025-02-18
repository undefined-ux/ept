use super::TStep;
use crate::types::interpretable::Interpretable;
use crate::types::steps::Permission;
use crate::{
    log,
    types::{
        mixed_fs::MixedFS,
        permissions::{Generalizable, PermissionLevel},
        verifiable::Verifiable,
        workflow::WorkflowContext,
    },
};
use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessExt, System, SystemExt};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StepKill {
    pub target: String,
}

fn kill(target: &String) -> Result<()> {
    let s = System::new_all();
    let mut count_suc = 0;
    let mut count_fail = 0;
    for process in s.processes_by_exact_name(target) {
        if process.kill() {
            count_suc += 1;
        } else {
            count_fail += 1;
        }
    }
    if count_suc + count_fail == 0 {
        log!("Warning(Kill):No process named '{target}' found.Tip for developer : note that field 'target' is case-sensitive and generally end with '.exe'");
    } else {
        log!("{level}(Kill):Killing '{target}' finished with {count_suc} succeeded, {count_fail} failed",level=if count_fail>0 {"Warning"}else{"Info"});
    }

    Ok(())
}

impl TStep for StepKill {
    fn run(self, _: &mut WorkflowContext) -> Result<i32> {
        kill(&self.target)?;

        Ok(0)
    }
    fn reverse_run(self, _: &mut WorkflowContext) -> Result<()> {
        Ok(())
    }
    fn get_manifest(&self, _: &mut MixedFS) -> Vec<String> {
        Vec::new()
    }
}

impl Interpretable for StepKill {
    fn interpret<F>(self, _: F) -> Self
    where
        F: Fn(String) -> String,
    {
        self
    }
}

impl Verifiable for StepKill {
    fn verify_self(&self, _: &String) -> Result<()> {
        if !self.target.to_lowercase().ends_with(".exe") {
            log!(
                "Warning(Kill):Generally field 'target' should end with '.exe', got '{t}'",
                t = self.target
            );
        }
        Ok(())
    }
}

impl Generalizable for StepKill {
    fn generalize_permissions(&self) -> Result<Vec<Permission>> {
        Ok(vec![Permission {
            key: "process_kill".to_string(),
            level: PermissionLevel::Sensitive,
            targets: vec![self.target.clone()],
        }])
    }
}

#[test]
fn test_kill() {
    use crate::types::workflow::WorkflowContext;
    envmnt::set("DEBUG", "true");
    envmnt::set("CONFIRM", "true");
    let mut cx = WorkflowContext::_demo();

    crate::utils::test::_ensure_clear_test_dir();
    std::fs::copy("examples/Notepad/Notepad/notepad.exe", "test/7zGM.exe").unwrap();
    crate::types::steps::StepExecute {
        command: "7zGM.exe".to_string(),
        pwd: Some("test".to_string()),
        call_installer: None,
        wait: Some("Abandon".to_string()),
    }
    .run(&mut cx)
    .unwrap();

    crate::types::steps::StepWait {
        timeout: 3000,
        break_if: None,
    }
    .run(&mut cx)
    .unwrap();

    StepKill {
        target: "7zGM.exe".to_string(),
    }
    .run(&mut cx)
    .unwrap();
}
