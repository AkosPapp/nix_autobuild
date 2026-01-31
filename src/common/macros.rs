#[macro_export]
macro_rules! generate_nixos_module {
    ($type:ty) => {
        $crate::paste::paste! {
            #[test]
            fn [<export_nixos_module_ $type:snake>]() -> std::io::Result<()> {
                use std::io::Write;

                let re = $crate::regex::Regex::new(r#"\n  (.*)Type = types\.submodule \{\n"#).unwrap();
                let mut nix = <$type>::nixos_type_full_definition();

                let replacements = re
                    .captures_iter(&nix)
                    .map(|cap| {
                        let from = format!("types.submodule {{ /* {} options */ }}", &cap[1]);
                        let type_name = &cap[1];
                        let to = format!(
                            "{}{}Type",
                            type_name.chars().next().unwrap().to_uppercase(),
                            &type_name[1..]
                        );
                        (from, to)
                    })
                    .collect::<Vec<_>>();
                for (from, to) in replacements {
                    nix = nix.replace(&from, &to);
                }

                // print current directory
                println!("CURRENT DIR: {:?}", std::env::current_dir());
                std::fs::create_dir_all("bindings")?;
                //panic!("Generate NixOS module failed");
                write!(
                    std::fs::File::create(&format!("bindings/{}.nix", <$type>::nixos_type_name()))?,
                    "{{lib, ...}}: let
  types = lib.types;
in {}",
                    nix.replace("types.submodule { /* Repo options */ }", "repoType")
                )

            }
        }
    };
}
