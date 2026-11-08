package net.kdt.pojavlaunch;

public final class CriticalNativeTest {
    private CriticalNativeTest() {
    }

    public static void invokeTest() {
        testCriticalNative(0, 0);
    }

    public static native void testCriticalNative(int a0, int a1);
}
