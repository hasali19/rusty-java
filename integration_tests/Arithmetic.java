package integration_tests;

class Arithmetic {
    static native void print(String v);

    static native void print(int v);

    public static void main(String[] args) {
        print("1 + 2 = ");
        print(add(1, 2));
    }

    private static int add(int a, int b) {
        return a + b;
    }
}
