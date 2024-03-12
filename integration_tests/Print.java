package integration_tests;

public class Print {
    private static native void print(String value);

    public static void main(String[] args) {
        print("Hello, world!");
    }
}
