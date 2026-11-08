package net.kdt.pojavlaunch.utils;

import android.content.Context;
import android.system.Os;

import java.io.File;
import java.util.ArrayList;
import java.util.List;

public final class JREUtils {
    private static volatile boolean bridgeLoaded = false;

    private JREUtils() {
    }

    public static synchronized void ensureBridgeLibrariesLoaded(File bridgeLibDir) {
        if (bridgeLoaded) {
            return;
        }
        File pojavExec = new File(bridgeLibDir, "libpojavexec.so");
        File byteHook = new File(bridgeLibDir, "libbytehook.so");
        File exitHook = new File(bridgeLibDir, "libexithook.so");
        File pojavAwt = new File(bridgeLibDir, "libpojavexec_awt.so");

        System.load(pojavExec.getAbsolutePath());
        String prevLd = null;
        try {
            prevLd = Os.getenv("LD_LIBRARY_PATH");
        } catch (Throwable ignored) {
        }
        String baseLd = (prevLd != null && !prevLd.trim().isEmpty())
            ? prevLd
            : (bridgeLibDir.getAbsolutePath() + ":/system/lib64:/vendor/lib64:/vendor/lib64/hw");
        try {
            Os.setenv("LD_LIBRARY_PATH", baseLd, true);
        } catch (Throwable ignored) {
        }
        setLdLibraryPath(baseLd);

        if (byteHook.isFile()) {
            System.load(byteHook.getAbsolutePath());
        }
        if (exitHook.isFile()) {
            System.load(exitHook.getAbsolutePath());
        }
        if (pojavAwt.isFile()) {
            System.load(pojavAwt.getAbsolutePath());
        }
        bridgeLoaded = true;
    }

    public static boolean isBridgeLoaded() {
        return bridgeLoaded;
    }

    public static File resolveJreLibArchDir(File jreHome) {
        File aarch64 = new File(jreHome, "lib/aarch64");
        if (aarch64.isDirectory()) {
            return aarch64;
        }
        return new File(jreHome, "lib");
    }

    public static List<File> locateSharedLibs(File root) {
        List<File> out = new ArrayList<>();
        File[] list = root.listFiles();
        if (list == null) {
            return out;
        }
        for (File f : list) {
            if (f.isFile() && f.getName().endsWith(".so")) {
                out.add(f);
            } else if (f.isDirectory()) {
                out.addAll(locateSharedLibs(f));
            }
        }
        return out;
    }

    public static void loadOpenJdkLibraries(File jreHome, File bridgeLibDir, File lwjglNativesDir) {
        File archLibDir = resolveJreLibArchDir(jreHome);
        File jliDir = new File(archLibDir, "jli");
        File serverDir = new File(archLibDir, "server");
        File clientDir = new File(archLibDir, "client");
        File jvmDir = serverDir.isDirectory() ? serverDir : clientDir;

        File jli = new File(jliDir, "libjli.so");
        if (!jli.isFile()) {
            jli = new File(archLibDir, "libjli.so");
        }
        if (jli.isFile()) {
            dlopen(jli.getAbsolutePath());
        }

        File jvm = new File(jvmDir, "libjvm.so");
        if (jvm.isFile()) {
            dlopen(jvm.getAbsolutePath());
        }

        String[] coreNames = {
            "libverify.so",
            "libjava.so",
            "libnet.so",
            "libnio.so",
            "libawt.so",
            "libawt_headless.so",
            "libfreetype.so",
            "libfontmanager.so"
        };
        for (String name : coreNames) {
            File f = new File(archLibDir, name);
            if (f.isFile()) {
                dlopen(f.getAbsolutePath());
            }
        }

        File customHeadless = new File(bridgeLibDir, "libawt_headless.so");
        if (customHeadless.isFile()) {
            dlopen(customHeadless.getAbsolutePath());
        }
        File customXawt = new File(bridgeLibDir, "libawt_xawt.so");
        if (customXawt.isFile()) {
            dlopen(customXawt.getAbsolutePath());
        }
        File lwjglFreetype = new File(lwjglNativesDir, "libfreetype.so");
        if (lwjglFreetype.isFile()) {
            dlopen(lwjglFreetype.getAbsolutePath());
        }

        for (File so : locateSharedLibs(archLibDir)) {
            dlopen(so.getAbsolutePath());
        }

        File openal = new File(bridgeLibDir, "libopenal.so");
        if (openal.isFile()) {
            dlopen(openal.getAbsolutePath());
        }
    }

    public static native int chdir(String path);

    public static native boolean dlopen(String libPath);

    public static native void setLdLibraryPath(String ldLibraryPath);

    public static native void setupBridgeWindow(Object surface);

    public static native void releaseBridgeWindow();

    public static native void initializeHooks();

    public static native void setupExitMethod(Context context);

    public static native int[] renderAWTScreenFrame();
}
