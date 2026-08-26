//! Procedural Material Studio desktop entry point.

use std::{env, path::PathBuf, process};

fn main() {
    let options = match parse(env::args().skip(1)) {
        Ok(Command::Run(options)) => options,
        Ok(Command::Help) => {
            print_help();
            return;
        }
        Err(error) => {
            eprintln!("island-material-studio: {error}");
            print_help();
            process::exit(2);
        }
    };
    let exit = island_material_studio::app::run(options);
    if let bevy::prelude::AppExit::Error(code) = exit {
        process::exit(i32::from(code.get()));
    }
}

enum Command {
    Run(island_material_studio::app::RunOptions),
    Help,
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut recipe_path = None;
    let mut window_size = None;
    let mut screenshot_path = None;
    let mut preview_tab = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--recipe" | "-r" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| format!("{argument} requires a file path"))?;
                recipe_path = Some(PathBuf::from(value));
            }
            "--window-size" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--window-size requires WIDTHxHEIGHT".to_owned())?;
                window_size = Some(parse_window_size(&value)?);
            }
            "--screenshot" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--screenshot requires a PNG path".to_owned())?;
                screenshot_path = Some(PathBuf::from(value));
            }
            "--preview-tab" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--preview-tab requires a tab name".to_owned())?;
                if ![
                    "albedo",
                    "height",
                    "normal",
                    "occlusion",
                    "packed_mask",
                    "layer_raw",
                    "layer_remapped",
                    "layer_mask",
                    "lit",
                ]
                .contains(&value.as_str())
                {
                    return Err(format!("unknown preview tab {value:?}"));
                }
                preview_tab = Some(value);
            }
            _ => return Err(format!("unknown option {argument:?}")),
        }
    }
    Ok(Command::Run(island_material_studio::app::RunOptions {
        recipe_path,
        window_size,
        screenshot_path,
        preview_tab,
    }))
}

fn parse_window_size(value: &str) -> Result<bevy::prelude::UVec2, String> {
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("invalid window size {value:?}; expected WIDTHxHEIGHT"))?;
    let width = width
        .parse::<u32>()
        .map_err(|_| format!("invalid window width {width:?}"))?;
    let height = height
        .parse::<u32>()
        .map_err(|_| format!("invalid window height {height:?}"))?;
    if width < 800 || height < 600 {
        return Err("window size must be at least 800x600".into());
    }
    Ok(bevy::prelude::UVec2::new(width, height))
}

fn print_help() {
    println!(
        "Procedural Material Studio\n\n\
         Usage: island-material-studio [OPTIONS]\n\n\
         Options:\n\
           -r, --recipe <FILE>  Open a JSON recipe at startup\n\
              --window-size <WIDTHxHEIGHT>  Override the persisted window size\n\
              --screenshot <PNG>  Capture the settled UI and exit (acceptance testing)\n\
              --preview-tab <TAB>  Select albedo, height, normal, occlusion, packed_mask, or lit\n\
           -h, --help           Show this help"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_optional_recipe() {
        let Command::Run(options) =
            parse(["--recipe".into(), "stone.json".into()].into_iter()).unwrap()
        else {
            panic!("expected run command");
        };
        assert_eq!(options.recipe_path, Some(PathBuf::from("stone.json")));
    }

    #[test]
    fn parses_acceptance_options() {
        let Command::Run(options) = parse(
            [
                "--window-size".into(),
                "1100x700".into(),
                "--screenshot".into(),
                "studio.png".into(),
            ]
            .into_iter(),
        )
        .unwrap() else {
            panic!("expected run command");
        };
        assert_eq!(
            options.window_size,
            Some(bevy::prelude::UVec2::new(1100, 700))
        );
        assert_eq!(options.screenshot_path, Some(PathBuf::from("studio.png")));
        assert_eq!(options.preview_tab, None);
    }
}
