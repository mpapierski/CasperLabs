class ExecutionEngine {
  // pointer to Rust object
  static class ExecutionEngineContext {
    protected long rustPrivPtr = 0;

    public ExecutionEngineContext(String dataDir) {
      ExecutionEngine.init(this, dataDir);
    }

    public void close() {
      if (this.rustPrivPtr != 0) {
        ExecutionEngine.destroy(this);
        this.rustPrivPtr = 0;
      }
    }
  }

  static class DeployCode {
    byte[] code;
    byte[] args;
  }

  static class Deploy {
    DeployCode session;
  }

  private static native void init(Object context, String data_dir);

  private static native void destroy(Object context);

  private static native void exec(Object context, byte[] parent_state_hash, long block_time, Deploy[] deploys,
      long protocol_version);

  static {
    System.loadLibrary("casperlabs_engine_server");
  }

  public static void main(String[] argv) {
    ExecutionEngineContext context = new ExecutionEngineContext("/tmp/data_dir");
    try {

      Deploy deploys[] = new Deploy[1];

      DeployCode sessionCode = new DeployCode();
      sessionCode.code = "abc".getBytes();
      sessionCode.args = "def".getBytes();

      deploys[0] = new Deploy();
      deploys[0].session = sessionCode;

      byte[] parent_state_hash = new byte[] { 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42,
          42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42 };
      ExecutionEngine.exec(context, parent_state_hash, 123456789, deploys, 1);
    } catch (Exception e) {
      System.out.print("Exception: ");
      System.out.println(e.getMessage());
    } finally {
      context.close();
    }
  }
}
