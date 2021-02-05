use std::fs;
use std::io::{self, Write};
use std::path::Path;
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};

pub fn generate(instance: &PackageInstance, source_root: &Path) -> io::Result<()> {
    let static_dir = source_root.join(&*instance.name);
    let mut share_dir = static_dir.join("usr/share");
    share_dir.push(&*instance.internal_config_sub_dir());
    let share_dir_internal = share_dir.join("internal_config");
    let mut config_dir = static_dir;
    config_dir.push("etc");
    config_dir.push(&*instance.config_sub_dir());
    for (file_name, conf) in instance.config() {
        let file_name = file_name.expand_to_cow(instance.constants_by_variant());
        if let ConfType::Static { content, internal } = &conf.conf_type {
            let file_path = if *internal {
                share_dir_internal.join(&*file_name)
            } else {
                config_dir.join(&*file_name)
            };
            fs::create_dir_all(file_path.parent().expect("file_path doesn't have a parent"))?;

            let mut file = fs::File::create(file_path)?;
            file.write_all(content.as_bytes())?;
        }
    }

    if let Some((_, db)) = instance.databases().iter().next() {
        let mut output = share_dir.join("dbconfig-common");
        fs::create_dir_all(&output)?;
        output.push("template");
        let mut template_file = fs::File::create(output)?;
        template_file.write_all(db.template.as_bytes())?;
    }

    Ok(())
}
