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
                        let first_char = &cap[1].chars().next().unwrap().to_uppercase().to_string();
                        let rest = &cap[1][first_char.len()..];

                        let type_name = format!("{}Type", &cap[1]);
                        let struct_name = format!("{}{}", first_char, rest);

                        let from = format!("types.submodule {{ /* {} options */ }}", struct_name);
                        let to = format!(
                            "(types.submodule {})",
                            type_name
                        );

                        let from2 = format!("  {} = types.submodule {{", type_name);
                        let to2 = format!("  {} = {{", type_name);

                        [
                        (from, to),
                        (from2, to2)
                        ]
                    })
                    .flatten()
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
                    nix
                )

            }
        }
    };
}
