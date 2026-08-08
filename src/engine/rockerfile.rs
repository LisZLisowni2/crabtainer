use std::fmt::format;
use std::fs;
use std::path::Path;
use nix::NixPath;

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Download { url: String, alias: String },
    From(String),
    Copy { src: String, dst: String },
    Run(String),
    Cmd(Vec<String>),
}

#[derive(Debug, Default)]
pub struct Rockerfile {
    pub instructions: Vec<Instruction>,
}

impl Rockerfile {
    pub fn parse_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Error reading file: {:?}", e))?;

        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self, String> {
        let mut instructions = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let keyword = parts.next().unwrap_or("").to_uppercase();
            let args = parts.next().unwrap_or("").trim();

            match keyword.as_str() {
                "DOWNLOAD" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: URL is required (DOWNLOAD)", line_num + 1));
                    }

                    let parts: Vec<&str> = args.split_whitespace().collect();
                    if parts.len() == 3 && parts[1].to_uppercase() == "AS" {
                        instructions.push(Instruction::Download {
                            url: parts[0].to_string(),
                            alias: parts[2].to_string()
                        })
                    } else {
                        return Err(format!("Line {}: DOWNLOAD syntax requires format: DOWNLOAD <URL> AS <ALIAS>", line_num + 1));
                    }
                },
                "FROM" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: Image name required (FROM)", line_num + 1));
                    }
                    instructions.push(Instruction::From(args.to_string()));
                },
                "COPY" => {
                    let copy_parts: Vec<&str> = args.split_whitespace().collect();
                    if copy_parts.len() != 2 {
                        return Err(format!("Line {}: Copy requires 2 arguments", line_num + 1));
                    }

                    instructions.push(Instruction::Copy {
                        src: copy_parts[0].to_string(),
                        dst: copy_parts[1].to_string(),
                    })
                },
                "RUN" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: Required argument (RUN)", line_num + 1));
                    }
                    instructions.push(Instruction::Run(args.to_string()));
                }
                "CMD" => {
                    if args.is_empty() {
                        return Err(format!("Line {}: Required argument (CMD)", line_num + 1));
                    }
                    let cmd_args = args.split_whitespace().map(|s| s.to_string()).collect();
                    instructions.push(Instruction::Cmd(cmd_args));
                }
                _ => {
                    return Err(format!("Line {}: Unknown keyword", line_num + 1));
                }
            }
        }

        Ok(Rockerfile { instructions })
    }
}