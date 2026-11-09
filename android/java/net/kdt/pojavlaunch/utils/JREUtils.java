package net.kdt.pojavlaunch.utils;

import android.content.Context;
import android.system.Os;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;

public final class JREUtils {
    static {
        try {
            System.loadLibrary("thk_jvm_shim");
        } catch (Throwable ignored) {
        }
    }

    private static volatile boolean bridgeLoaded = false;
    private static volatile String exitHookLibPath = null;

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
        safeSetLdLibraryPath(baseLd);

        if (byteHook.isFile()) {
            dlopen(byteHook.getAbsolutePath());
        }
        if (exitHook.isFile()) {
            String path = exitHook.getAbsolutePath();
            if (dlopen(path)) {
                exitHookLibPath = path;
            }
        }
        if (pojavAwt.isFile()) {
            System.load(pojavAwt.getAbsolutePath());
        }
        bridgeLoaded = true;
    }

    public static boolean safeSetLdLibraryPath(String ldLibraryPath) {
        if (ldLibraryPath == null || ldLibraryPath.isEmpty()) {
            return false;
        }
        try {
            return nativeSafeSetLdLibraryPath(ldLibraryPath);
        } catch (UnsatisfiedLinkError ignored) {
            return false;
        }
    }

    public static void initializeHooks() {
        String path = exitHookLibPath;
        if (path == null || path.isEmpty()) {
            return;
        }
        try {
            nativeInvokeExitHookInit(path);
        } catch (UnsatisfiedLinkError ignored) {
        }
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
        Arrays.sort(list, Comparator.comparing(File::getName));
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
        String[] bridgePrereqs = {
            "libc++_shared.so",
            "libspirv-cross-c-shared.so"
        };
        if (bridgeLibDir != null) {
            for (String name : bridgePrereqs) {
                File f = new File(bridgeLibDir, name);
                if (f.isFile()) {
                    dlopen(f.getAbsolutePath());
                }
            }
        }

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
            "libzip.so",
            "libjimage.so",
            "libprefs.so",
            "libmanagement.so",
            "libawt.so"
        };
        for (String name : coreNames) {
            File f = new File(archLibDir, name);
            if (f.isFile()) {
                dlopen(f.getAbsolutePath());
            }
        }

        File lwjglFreetype = lwjglNativesDir != null ? new File(lwjglNativesDir, "libfreetype.so") : null;
        boolean hasLwjglFreetype = lwjglFreetype != null && lwjglFreetype.isFile();
        if (hasLwjglFreetype) {
            dlopen(lwjglFreetype.getAbsolutePath());
        } else {
            File jreFreetype = new File(archLibDir, "libfreetype.so");
            if (jreFreetype.isFile()) {
                dlopen(jreFreetype.getAbsolutePath());
            }
        }

        File jreHeadless = new File(archLibDir, "libawt_headless.so");
        if (jreHeadless.isFile()) {
            dlopen(jreHeadless.getAbsolutePath());
        } else if (bridgeLibDir != null) {
            File customHeadless = new File(bridgeLibDir, "libawt_headless.so");
            if (customHeadless.isFile()) {
                dlopen(customHeadless.getAbsolutePath());
            }
        }
        if (bridgeLibDir != null) {
            File customXawt = new File(bridgeLibDir, "libawt_xawt.so");
            if (customXawt.isFile()) {
                ensureJreAwtXawtStub(archLibDir, customXawt);
                dlopen(customXawt.getAbsolutePath());
            }
        }

        File fontManager = new File(archLibDir, "libfontmanager.so");
        if (fontManager.isFile()) {
            dlopen(fontManager.getAbsolutePath());
        }

        List<File> jreSharedLibs = locateSharedLibs(archLibDir);
        Collections.sort(jreSharedLibs, Comparator.comparing(File::getAbsolutePath));
        for (File so : jreSharedLibs) {
            String lowerName = so.getName().toLowerCase(Locale.ROOT);
            if ("libawt_headless.so".equals(lowerName)
                || "libawt_xawt.so".equals(lowerName)
                || "libjsig.so".equals(lowerName)
                || (hasLwjglFreetype && "libfreetype.so".equals(lowerName))) {
                continue;
            }
            dlopen(so.getAbsolutePath());
        }

        if (bridgeLibDir != null) {
            File openal = new File(bridgeLibDir, "libopenal.so");
            if (openal.isFile()) {
                dlopen(openal.getAbsolutePath());
            }
        }

        if (lwjglNativesDir != null && lwjglNativesDir.isDirectory()) {
            String[] lwjglOrder = {
                "liblwjgl.so",
                "liblwjgl_opengl.so",
                "liblwjgl_stb.so",
                "liblwjgl_tinyfd.so",
                "liblwjgl_nanovg.so",
                "liblwjgl_vma.so",
                "liblwjgl_spvc.so",
                "liblwjgl_spng.so",
                "liblwjgl_freetype.so"
            };
            for (String name : lwjglOrder) {
                File f = new File(lwjglNativesDir, name);
                if (f.isFile()) {
                    dlopen(f.getAbsolutePath());
                }
            }
            for (File so : locateSharedLibs(lwjglNativesDir)) {
                if ("libfreetype.so".equalsIgnoreCase(so.getName())) {
                    continue;
                }
                dlopen(so.getAbsolutePath());
            }
        }
    }

    private static void ensureJreAwtXawtStub(File archLibDir, File customXawt) {
        if (archLibDir == null || customXawt == null || !archLibDir.isDirectory() || !customXawt.isFile()) {
            return;
        }
        File jreXawt = new File(archLibDir, "libawt_xawt.so");
        try {
            if (jreXawt.exists()) {
                String canonTarget = customXawt.getCanonicalPath();
                String canonExisting = jreXawt.getCanonicalPath();
                if (canonTarget.equals(canonExisting)) {
                    return;
                }
                //noinspection ResultOfMethodCallIgnored
                jreXawt.delete();
            }
        } catch (Throwable ignored) {
            //noinspection ResultOfMethodCallIgnored
            jreXawt.delete();
        }
        try {
            Os.symlink(customXawt.getAbsolutePath(), jreXawt.getAbsolutePath());
            return;
        } catch (Throwable ignored) {
        }
        File tmp = new File(archLibDir, "libawt_xawt.so.tmp");
        try (FileInputStream in = new FileInputStream(customXawt);
             FileOutputStream out = new FileOutputStream(tmp)) {
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) != -1) {
                out.write(buf, 0, n);
            }
            out.flush();
            Os.chmod(tmp.getAbsolutePath(), 0500);
            //noinspection ResultOfMethodCallIgnored
            tmp.renameTo(jreXawt);
        } catch (Throwable ignored) {
            //noinspection ResultOfMethodCallIgnored
            tmp.delete();
        }
    }

    public static native int chdir(String path);

    public static native boolean dlopen(String libPath);

    public static native void setLdLibraryPath(String ldLibraryPath);

    public static native void setupBridgeWindow(Object surface);

    public static native void releaseBridgeWindow();

    private static native boolean nativeSafeSetLdLibraryPath(String ldLibraryPath);

    private static native boolean nativeInvokeExitHookInit(String exitHookPath);

    public static native void setupExitMethod(Context context);

    public static native int[] renderAWTScreenFrame();
}
