package integration_tests;

/**
 * SystemTime
 */
public class SystemTime {

    private static native void print(String value);

    private static native void print(long value);

    public static void main(String[] args) {
        print("Current time: ");
        print(System.currentTimeMillis());
    }
}
