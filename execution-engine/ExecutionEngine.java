class ExecutionEngine {
  // pointer to Rust object
  protected long rustPrivPtr = 0;

  private static native void init(Object context, String data_dir);

  private static native void destroy(Object context);

  static {
    System.loadLibrary("casperlabs_engine_server");
  }

  public static void main(String[] argv) {
    ExecutionEngine context = new ExecutionEngine();
    ExecutionEngine.init(context, "/tmp/data_dir/");
    ExecutionEngine.destroy(context);
  }

}
