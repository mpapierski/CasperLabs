pub struct DeployCode {
    /// wasm byte code
    code: Vec<u8>,
    /// ABI-encoded arguments
    args: Vec<u8>,
}
