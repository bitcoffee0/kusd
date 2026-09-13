use serde::Serialize;

/// Compiled covenant bytecode split around its serialized state.
#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub bytecode: Vec<u8>,
    pub prefix: Vec<u8>,
    pub suffix: Vec<u8>,
    pub template_hash: Vec<u8>,
}
