package com.trihienkun.laucher;

import android.content.Context;
import android.view.Surface;

public final class ThkNativeBridge {
    static {
        System.loadLibrary("thk_android_ffi");
    }

    private ThkNativeBridge() {
    }

    public static void ensureLoaded() {
    }

    public static native void nativeInitHost(
        MainActivity activity,
        String filesDir,
        String apkSourceDir,
        String socHint
    );

    public static native void nativeShutdownHost();

    public static native void nativeSyncHostState(
        boolean ignoreBatteryOptimizations,
        boolean canDrawOverlays,
        boolean notificationsEnabled,
        boolean keepAliveRunning,
        boolean overlayShowing,
        boolean surfaceReady,
        int surfaceWidth,
        int surfaceHeight,
        boolean gameProcessRunning,
        int safeInsetLeftPx,
        int safeInsetTopPx,
        int safeInsetRightPx,
        int safeInsetBottomPx
    );

    public static native void nativeSyncNetworkFromJava(
        String[] ifacePairs,
        String[] globalIpv6,
        String[] dnsServers
    );

    public static native String nativeReadSavedOrientation(String filesDir);

    public static native String nativeResolveLaunchOrientation(
        String filesDir,
        int windowWidth,
        int windowHeight
    );

    public static native void nativeWriteFocusMode(String filesDir, String mode);

    public static native String nativeStagePickedFd(
        String filesDir,
        int detachedFd,
        String displayName
    );

    public static native void nativeCompletePickRequest(
        String filesDir,
        String reqId,
        String[] stagedPaths
    );

    public static native void nativeStartGameJvm(
        Context appContext,
        String filesDir,
        String apkSourceDir,
        String specPath,
        Surface initialSurface,
        int width,
        int height,
        String[] dnsServers
    );

    public static native void nativeUpdateGameSurface(
        Surface surface,
        int width,
        int height
    );

    public static native void nativeStopGameJvm();

    public static native void nativeNotifyGrabState(boolean grabbing);

    public static native void nativeNotifyJvmExit(int code);
}
