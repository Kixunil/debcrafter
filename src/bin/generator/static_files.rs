use std::fs;
use std::io::{self, Write};
use std::path::Path;
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};

pub fn generate(instance: &PackageInstance, source_root: &Path) -> io::Result<()> {
    let static_dir = source_root.join(&*instance.name);
    let mut share_dir = static_dir.join("usr/share");
    share_dir.push(&*instance.internal_config_sub_dir());
    share_dir.push("internal_config");
    let mut config_dir = static_dir;
    config_dir.push("etc");
    config_dir.push(&*instance.config_sub_dir());
    for (file_name, conf) in instance.config() {
        if let ConfType::Static { content, internal } = &conf.conf_type {
            let file_path = if *internal {
                share_dir.join(file_name)
            } else {
                config_dir.join(file_name)
            };
            fs::create_dir_all(file_path.parent().expect("file_path doesn't have a parent"))?;

            let mut file = fs::File::create(file_path)?;
            file.write_all(content.as_bytes())?;

            if *internal {
                let config_file_path = config_dir.join(file_name);
                fs::create_dir_all(config_file_path.parent().expect("file_path doesn't have a parent"))?;
                let mut share_relative = std::path::PathBuf::from("../../usr/share");
                share_relative.push(&*instance.internal_config_sub_dir());
                share_relative.push("internal_config");
                share_relative.push(file_name);
                std::os::unix::fs::symlink(share_relative, config_file_path)?;
            }
        }
    }

    Ok(())
}
