package integration_tests;

public class Arrays {
    private static native void print(int[] vs);

    public static void main(String[] args) {
        int[] integers = new int[10];

        for (int i = 0; i < integers.length; i++) {
            integers[i] = i + 1;
        }

        print(integers);
    }
}
