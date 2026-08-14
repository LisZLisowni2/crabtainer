#[derive(Debug)]
pub struct ContainerOptions {
    pub layout_name: String,
    pub args: Vec<String>,
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<f64>,
    pub detach: bool,
}

#[derive(Debug)]
pub struct ContainerReady {
    pub layout_name: String,
    pub args: Vec<String>,
    pub quota: Option<i64>,
    pub memory_limit: Option<i64>,
}
