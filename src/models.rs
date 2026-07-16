use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct PythonPackage {
    pub name: String,
    pub modules: Vec<PythonModule>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonModule {
    pub name: String,
    pub filepath: String,
    pub docstring: Option<String>,
    pub classes: Vec<PythonClass>,
    pub functions: Vec<PythonFunction>,
    pub z_index: i32,
    pub folder: Option<String>,
    pub link_path: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonClass {
    pub name: String,
    pub docstring: Option<String>,
    pub functions: Vec<PythonFunction>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonFunction {
    pub name: String,
    pub args: Vec<String>,
    pub return_type: Option<String>,
    pub docstring: Option<String>,
}