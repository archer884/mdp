use core::slice;
use std::{env, fs, io, path::Path, process::Command};

use clap::Clap;
use serde::Deserialize;

// Fun fact: I have basically zero intention of using this facility.
#[derive(Clap, Clone, Debug)]
struct Opts {
    /// directory containing pandoc source files
    path: Option<String>,

    /// outputs, e.g. foo.docx, foo.pdf, etc.
    outputs: Vec<String>,

    /// the output directory; generated outputs will be
    /// placed in here
    #[clap(long = "out-directory")]
    out_directory: Option<String>,

    /// styling document
    #[clap(long = "reference-doc")]
    reference_doc: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Configuration {
    // We'll have a conventional name (pub?) for the output
    // directory, but it can be overridden.
    out_directory: Option<String>,
    #[serde(rename = "task")]
    tasks: Vec<Task>,
}

impl Configuration {
    /// Combine configuration with command line options.
    fn with_opts(mut self, opts: Opts) -> Configuration {
        // The idea here is to take in the options object and return a new
        // configuration object wherein opts values have been allowed to
        // override configuration values.
        Configuration {
            out_directory: opts.out_directory.or(self.out_directory),
            tasks: match opts.path {
                // If the user has provided a path, it will override any
                // configured tasks.
                Some(path) => vec![Task {
                    source: path,
                    outputs: opts.outputs,
                }],

                // If the user has not provided a path, tasks will be drawn
                // from configuration OR by convention, but outputs can still
                // be overridden by configuration.
                None => {
                    if self.tasks.is_empty() {
                        vec![Task {
                            source: String::from("src"),
                            outputs: opts.outputs,
                        }]
                    } else {
                        let outputs = opts.outputs;
                        if !outputs.is_empty() {
                            self.tasks
                                .iter_mut()
                                .for_each(|task| task.outputs = outputs.clone());
                        }
                        self.tasks
                    }
                }
            },
        }
    }

    fn tasks(&self) -> slice::Iter<Task> {
        self.tasks.iter()
    }

    fn out_directory(&self) -> &Path {
        self.out_directory
            .as_ref()
            .map(Path::new)
            .unwrap_or_else(|| Path::new("pub"))
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Task {
    // Source has a one to many relationship with outputs;
    // a book may be configured to produce both a docx and
    // epub output.
    source: String,
    outputs: Vec<String>,
}

fn main() -> io::Result<()> {
    // WARNING: THE FOLLOWING COMMAND DOES NOT WORK, BECAUSE
    // PANDOC ALLOWS FOR ONLY A SINGLE OUTPUT AT A TIME.
    // let command = Command::new("pandoc")
    //     .arg("./README.md")
    //     .args(&["-o", "readme.docx", "-o", "readme.epub"])
    //     .output()
    //     .unwrap();

    let configuration = read_configuration()?.with_opts(Opts::parse());

    Ok(())
}

fn read_configuration() -> io::Result<Configuration> {
    let current_dir = env::current_dir()?;
    let config_path = current_dir.join(".mdb.conf");

    if !config_path.exists() {
        Ok(Configuration::default())
    } else {
        let config = fs::read_to_string(config_path)?;
        let config: Configuration = toml::from_str(&config)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use crate::Configuration;

    #[test]
    fn can_deserialize_configuration() {
        let configuration: Configuration =
            toml::from_str(include_str!("../resource/test.toml")).unwrap();

        assert!(configuration.out_directory.is_some());
        assert_eq!(
            configuration.out_directory.unwrap(),
            "non-standard directory"
        );
        assert_eq!(configuration.tasks.len(), 1);
        assert_eq!(configuration.tasks[0].source, "masquerade");
        assert_eq!(configuration.tasks[0].outputs.len(), 2);
    }
}
