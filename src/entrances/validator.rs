use std::path::Path;
use anyhow::{anyhow,Result};

pub fn inner_validator(dir:String)->Result<()>{
    let manifest = vec!["package.toml", "workflows/setup.toml","workflows/remove.toml"];
    for file_name in manifest {
        let p = Path::new(&dir).join(file_name);
        if !p.exists() {
            return Err(anyhow!(
                "Error:Missing '{}' in '{}', can't pack to nep",
                &file_name,
                &dir
            ));
        }
    }
    Ok(())
}

// 返回内包路径
pub fn outer_validator(dir:String,stem:String)->Result<String>{
    let inner_pkg_name=stem+".tar.zst";
    let manifest = vec!["package.toml", &inner_pkg_name];
    for file_name in manifest {
        let p = Path::new(&dir).join(file_name);
        if !p.exists() {
            return Err(anyhow!(
                "Error:Invalid nep package : missing '{}' in '{}'",
                &file_name,
                &dir
            ));
        }
    }

    let inner_path=Path::new(&dir).join(&inner_pkg_name);
    Ok(inner_path.to_string_lossy().to_string())
}

// 返回上下文目录路径
pub fn installed_validator(dir:String)->Result<String>{
    let ctx_path=Path::new(&dir).join(".nep_context");
    if !ctx_path.exists()||ctx_path.is_file(){
        return Err(anyhow!("Error:Invalid nep app folder : missing '.nep_context' folder in '{}'",&dir));
    }

    let ctx_str=ctx_path.to_string_lossy().to_string();
    inner_validator(ctx_str.clone())?;
    Ok(ctx_str)
}