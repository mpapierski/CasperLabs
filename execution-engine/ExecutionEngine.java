class ExecutionEngine {
  // pointer to Rust object
  protected long rustPrivPtr = 0;

  static class DeployCode {
    byte[] code;
    byte[] args;
  }
  static class Deploy {
    DeployCode session;
  }

  private static native void init(Object context, String data_dir);

  private static native void destroy(Object context);

  private static native void exec(Object context,
    byte[] parent_state_hash,
    long block_time,
    Deploy[] deploys,
    long protocol_version);

  static {
    System.loadLibrary("casperlabs_engine_server");
  }

  public static void main(String[] argv) {
    ExecutionEngine context = new ExecutionEngine();
    ExecutionEngine.init(context, "/tmp/data_dir/");

    Deploy deploys[] = new Deploy[1];

    DeployCode sessionCode = new DeployCode();
    sessionCode.code = "abc".getBytes();
    sessionCode.args = "def".getBytes();

    deploys[0] = new Deploy();
    deploys[0].session = sessionCode;

    byte[] parent_state_hash = new byte[]{42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42};
    ExecutionEngine.exec(context, parent_state_hash, 123456789, deploys, 1);

    ExecutionEngine.destroy(context);
  }

}
