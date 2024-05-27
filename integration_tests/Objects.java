package integration_tests;

public class Objects {
    private static native void print(Object v);

    public static void main(String[] args) {
        var obj = new MyClass();
        obj.x = 123;
        obj.y = true;
        obj.z = new int[] { 1, 2, 3 };
        print(obj);
    }

    private static class MyClass {
        public int x;
        public boolean y;
        public int[] z;
    }
}
