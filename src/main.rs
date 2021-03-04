use std::{
    env,
    fs::{self, File},
    io::{self, Cursor},
    iter::FromIterator,
    path::{Path, PathBuf},
    process::{self, Command},
    slice,
};

use chrono::{DateTime, Utc};
use clap::Clap;
use serde::{Deserialize, Serialize};

type FileTime = DateTime<Utc>;

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
    out_directory: Option<String>,
    #[serde(rename = "task")]
    tasks: Vec<Task>,
    reference_doc: Option<String>,
}

impl Configuration {
    /// Combine configuration with command line options.
    fn with_opts(mut self, opts: Opts) -> io::Result<RuntimeConfiguration> {
        let current_dir = env::current_dir()?;
        let configuration = Configuration {
            out_directory: opts.out_directory.or(self.out_directory),
            tasks: match opts.path {
                // If the user has provided a path, it will override any
                // configured tasks.
                Some(path) => vec![Task {
                    source: path,
                    outputs: opts.outputs.into_iter().map(PathBuf::from).collect(),
                }],

                // If the user has not provided a path, tasks will be drawn
                // from configuration OR by convention, but outputs can still
                // be overridden by configuration.
                None => {
                    if self.tasks.is_empty() {
                        let mut outputs: Vec<_> =
                            opts.outputs.into_iter().map(PathBuf::from).collect();
                        if outputs.is_empty() {
                            if let Some(parent) = current_dir.parent() {
                                // I don't see how this can fail.
                                let stripped = current_dir.strip_prefix(parent).unwrap();
                                outputs.push(stripped.with_extension("docx"))
                            }
                        }
                        vec![Task {
                            source: String::from("src"),
                            outputs,
                        }]
                    } else {
                        let outputs: Vec<_> = opts.outputs.into_iter().map(PathBuf::from).collect();
                        if !outputs.is_empty() {
                            self.tasks
                                .iter_mut()
                                .for_each(|task| task.outputs = outputs.clone());
                        }
                        self.tasks
                    }
                }
            },
            reference_doc: opts.reference_doc,
        };

        Ok(RuntimeConfiguration {
            current_dir,
            configuration,
        })
    }
}

struct RuntimeConfiguration {
    current_dir: PathBuf,
    configuration: Configuration,
}

impl RuntimeConfiguration {
    fn tasks(&self) -> slice::Iter<Task> {
        self.configuration.tasks.iter()
    }

    fn source_path(&self, source: impl AsRef<Path>) -> PathBuf {
        self.current_dir.join(source)
    }

    fn build_path(&self, source: impl AsRef<Path>) -> PathBuf {
        self.configuration
            .out_directory
            .as_ref()
            .map(Path::new)
            .unwrap_or_else(|| Path::new("pub"))
            .join(source)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Task {
    // Source has a one to many relationship with outputs;
    // a book may be configured to produce both a docx and
    // epub output.
    source: String,
    outputs: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Snapshot(Vec<(PathBuf, FileTime)>);

impl Snapshot {
    fn args(&self) -> Vec<&Path> {
        self.0.iter().map(|x| x.0.as_ref()).collect()
    }
}

impl FromIterator<(PathBuf, FileTime)> for Snapshot {
    fn from_iter<T: IntoIterator<Item = (PathBuf, FileTime)>>(iter: T) -> Self {
        let mut inner: Vec<_> = iter.into_iter().collect();
        inner.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Snapshot(inner)
    }
}

fn main() -> io::Result<()> {
    let configuration = read_configuration()?.with_opts(Opts::parse())?;
    for task in configuration.tasks() {
        execute_task(task, &configuration)?;
    }

    Ok(())
}

fn execute_task(task: &Task, configuration: &RuntimeConfiguration) -> io::Result<()> {
    let constituent_files = list_files(configuration.source_path(&task.source))?;
    let build_path = configuration.build_path(&task.source);

    for output in &task.outputs {
        let target_path = build_target_path(&build_path, &output)?;
        let output_file_path = target_path.join(output);

        if !target_path.exists() {
            fs::create_dir_all(&target_path)?;
        }

        let snapshot_path = target_path.join(".snapshot");
        let snapshot = load_build_snapshot(&snapshot_path)?;
        if output_file_path.exists() && !should_rebuild(snapshot.as_ref(), &constituent_files) {
            continue;
        }

        let mut command = Command::new("pandoc");
        command
            .args(&constituent_files.args())
            .arg("-o")
            .arg(&output_file_path);

        if let Some(reference_doc) = try_get_reference_doc(&configuration)? {
            command.arg("--reference-doc").arg(reference_doc);
        }

        let result = command.output()?;
        if result.status.success() {
            println!("{}", output.display());
            let mut snapshot_file = File::create(snapshot_path)?;
            serde_json::to_writer_pretty(&mut snapshot_file, &constituent_files)?;
        } else {
            eprintln!("Failed to generate {}", output.display());
            let stderr = io::stderr();
            let mut stderr = stderr.lock();
            io::copy(&mut Cursor::new(result.stderr), &mut stderr)?;
            process::exit(1);
        }
    }

    Ok(())
}

// This looks like a mess to me, but maybe it'll work.
fn try_get_reference_doc(configuration: &RuntimeConfiguration) -> io::Result<Option<PathBuf>> {
    if let Some(path) = &configuration.configuration.reference_doc {
        let path = Path::new(path);
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "missing reference document",
            ));
        }

        Ok(Some(path.into()))
    } else {
        let path = configuration.current_dir.join("style/style.docx");
        if path.exists() {
            return Ok(Some(path));
        }
        Ok(None)
    }
}

fn should_rebuild(snapshot: Option<&Snapshot>, constituent_files: &Snapshot) -> bool {
    snapshot
        .map(|snapshot| snapshot != constituent_files)
        .unwrap_or(true)
}

fn list_files(path: impl AsRef<Path>) -> io::Result<Snapshot> {
    Ok(fs::read_dir(path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let metadata = entry.metadata().ok()?;
            if metadata.is_file() {
                Some((entry.path(), metadata.modified().ok()?.into()))
            } else {
                None
            }
        })
        .collect())
}

fn build_target_path(build_path: &Path, output: &Path) -> io::Result<PathBuf> {
    let output_filename = Path::new(output);
    let extension = output_filename.extension().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "output lacks valid filename extension",
        )
    })?;
    Ok(build_path.join(extension))
}

fn load_build_snapshot(snapshot_path: &Path) -> io::Result<Option<Snapshot>> {
    if snapshot_path.exists() {
        let file = File::open(snapshot_path)?;
        Ok(Some(serde_json::from_reader(file)?))
    } else {
        Ok(None)
    }
}

fn read_configuration() -> io::Result<Configuration> {
    let current_dir = env::current_dir()?;
    let config_path = current_dir.join("mdp.toml");

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
