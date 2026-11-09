package net.kdt.pojavlaunch.utils;

import android.content.Context;

public final class JREUtils {
    static {
        com.trihienkun.laucher.ThkNativeBridge.ensureLoaded();
    }

    private JREUtils() {
    }

    public static void loadPojavExec(String absPath) {
        System.load(absPath);
    }

    public static void loadPojavAwt(String absPath) {
        System.load(absPath);
    }

    public static native int chdir(String path);

    public static native boolean dlopen(String libPath);

    public static native void setLdLibraryPath(String ldLibraryPath);

    public static native void setupBridgeWindow(Object surface);

    public static native void releaseBridgeWindow();

    public static native void setupExitMethod(Context context);

    public static native int[] renderAWTScreenFrame();
}
