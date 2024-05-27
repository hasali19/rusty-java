package integration_tests;

public class Objects {
    private static native void print(Object v);

    public static void main(String[] args) {
        var obj = new MyClass(123, true, new int[] { 1, 2, 3 });
        print(obj);
        print("\n");

        obj.incrementX();
        print(obj);
        print("\n");
    }

    private static class MyClass {
        public int x;
        public boolean y;
        public int[] z;

        public MyClass(int x, boolean y, int[] z) {
            this.x = x;
            this.y = y;
            this.z = z;
        }

        public void incrementX() {
            x++;
        }
    }
}
