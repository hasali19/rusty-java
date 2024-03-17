package integration_tests;

/**
 * StaticClass
 */
public class StaticClass {

    public static void main(String[] args) {
        Inner.print("Hello from inner class\n");
    }

    private static class Inner {
        public static native void print(String s);
    }
}
