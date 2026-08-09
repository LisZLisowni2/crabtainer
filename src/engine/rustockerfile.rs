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
pub struct Rustockerfile {
    pub instructions: Vec<Instruction>,
}

impl Rustockerfile {
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

        Ok(Rustockerfile { instructions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_instruction_types() {
        let content = "\
FROM alpine-base
DOWNLOAD https://example.com/alpine.tar.gz AS alpine-base
COPY src /app
RUN echo hello
CMD /bin/sh -c
";
        let parsed = Rustockerfile::parse(content).unwrap();
        assert_eq!(parsed.instructions, vec![
            Instruction::From("alpine-base".to_string()),
            Instruction::Download {
                url: "https://example.com/alpine.tar.gz".to_string(),
                alias: "alpine-base".to_string(),
            },
            Instruction::Copy { src: "src".to_string(), dst: "/app".to_string() },
            Instruction::Run("echo hello".to_string()),
            Instruction::Cmd(vec!["/bin/sh".to_string(), "-c".to_string()]),
        ]);
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let parsed = Rustockerfile::parse("from ubuntu\ncopy a /b\nrun ls\n").unwrap();
        assert_eq!(parsed.instructions, vec![
            Instruction::From("ubuntu".to_string()),
            Instruction::Copy { src: "a".to_string(), dst: "/b".to_string() },
            Instruction::Run("ls".to_string()),
        ]);
    }

    #[test]
    fn ignores_blank_lines_and_comments() {
        let content = "\n# header comment\nFROM alpine\n\n# another comment\nRUN true\n";
        let parsed = Rustockerfile::parse(content).unwrap();
        assert_eq!(parsed.instructions.len(), 2);
    }

    #[test]
    fn empty_file_yields_no_instructions() {
        let parsed = Rustockerfile::parse("\n  \n# only a comment\n").unwrap();
        assert!(parsed.instructions.is_empty());
    }

    #[test]
    fn unknown_keyword_reports_line_number() {
        let err = Rustockerfile::parse("FROM alpine\nMAYBE ls\n").unwrap_err();
        assert!(err.contains("Line 2"), "unexpected error: {}", err);
        assert!(err.contains("Unknown keyword"));
    }

    #[test]
    fn from_requires_argument() {
        let err = Rustockerfile::parse("FROM\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("FROM"));
    }

    #[test]
    fn download_requires_as_syntax() {
        let err = Rustockerfile::parse("DOWNLOAD https://example.com/x.tar.gz\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("DOWNLOAD <URL> AS <ALIAS>"));

        let err = Rustockerfile::parse("DOWNLOAD https://example.com/x.tar.gz WRONG alias\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("DOWNLOAD <URL> AS <ALIAS>"));
    }

    #[test]
    fn copy_requires_exactly_two_arguments() {
        let err = Rustockerfile::parse("COPY only_one\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("2 arguments"));

        let err = Rustockerfile::parse("COPY a b c\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("2 arguments"));
    }

    #[test]
    fn run_requires_argument() {
        let err = Rustockerfile::parse("RUN\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("RUN"));
    }

    #[test]
    fn cmd_requires_argument() {
        let err = Rustockerfile::parse("CMD\n").unwrap_err();
        assert!(err.contains("Line 1"));
        assert!(err.contains("CMD"));
    }

    #[test]
    fn preserves_run_command_including_spaces() {
        let parsed = Rustockerfile::parse("RUN apk add --no-cache curl && echo done\n").unwrap();
        assert_eq!(parsed.instructions, vec![
            Instruction::Run("apk add --no-cache curl && echo done".to_string()),
        ]);
    }

    #[test]
    fn parse_from_file_reads_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Rustockerfile");
        std::fs::write(&path, "FROM ubuntu\n").unwrap();

        let parsed = Rustockerfile::parse_from_file(&path).unwrap();
        assert_eq!(parsed.instructions, vec![Instruction::From("ubuntu".to_string())]);
    }

    #[test]
    fn parse_from_file_missing_file_errors() {
        let err = Rustockerfile::parse_from_file("/does/not/exist/Rustockerfile").unwrap_err();
        assert!(err.contains("Error reading file"));
    }
}