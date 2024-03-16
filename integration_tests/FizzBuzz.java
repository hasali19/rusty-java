package integration_tests;

class FizzBuzz {
    private static native void print(String s);

    private static native void print(int i);

    public static void main(String[] args) {
        for (int i = 1; i <= 100; i++) {
            if (i % 3 == 0) {
                if (i % 5 == 0) {
                    print("FizzBuzz\n");
                } else {
                    print("Fizz\n");
                }
            } else {
                if (i % 5 == 0) {
                    print("Buzz\n");
                } else {
                    print(i);
                    print("\n");
                }
            }
        }
    }
}
