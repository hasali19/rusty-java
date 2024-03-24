package integration_tests;

public class StaticFields {
    private static int x = 42;

    private static native void print(String v);

    private static native void print(int v);

    public static void main(String[] args) {
        print("The answer to life, the universe and everything is: ");
        print(x);
    }
}
