use std::fs;
use std::io::{self, Write};
use std::path::Path;
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};

pub fn generate(instance: &PackageInstance, source_root: &Path) -> io::Result<()> {
    let mut config_dir = source_root.join(&*instance.name);
    config_dir.push("etc");
    config_dir.push(&*instance.config_sub_dir());
    for (file_name, conf) in instance.config() {
        if let ConfType::Static { content } = &conf.conf_type {
            let file_path = config_dir.join(file_name);
            fs::create_dir_all(file_path.parent().expect("file_path doesn't have a parent"))?;

            let mut file = fs::File::create(file_path)?;
            file.write_all(content.as_bytes())?;
        }
    }

    Ok(())
}
