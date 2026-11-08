package com.trihienkun.laucher;

import android.Manifest;
import android.app.Activity;
import android.app.NativeActivity;
import android.app.NotificationManager;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.database.Cursor;
import android.graphics.Insets;
import android.graphics.PixelFormat;
import android.graphics.Rect;
import android.graphics.Region;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.Uri;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Looper;
import android.os.Message;
import android.os.Messenger;
import android.os.PowerManager;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.system.Os;
import android.view.DisplayCutout;
import android.view.RoundedCorner;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.FrameLayout;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.net.Inet4Address;
import java.net.Inet6Address;
import java.net.InetAddress;
import java.net.NetworkInterface;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Enumeration;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

public final class MainActivity extends NativeActivity {
    private static final int REQ_PICK_DOCUMENT = 4101;
    private static final int REQ_POST_NOTIFICATIONS = 4102;

    private final Object pickLock = new Object();
    private final Object gameLock = new Object();

    private volatile boolean bridgeRunning = false;
    private volatile boolean appInForeground = false;
    private Thread bridgeThread;
    private String activePickId;
    private WifiManager.MulticastLock multicastLock;

    private SurfaceView gameSurfaceView;
    private Surface activeSurface;
    private boolean surfaceReady = false;
    private int surfacePhysWidth = 0;
    private int surfacePhysHeight = 0;
    private volatile int safeInsetLeftPx = 0;
    private volatile int safeInsetTopPx = 0;
    private volatile int safeInsetRightPx = 0;
    private volatile int safeInsetBottomPx = 0;

    private Messenger gameServiceMessenger;
    private boolean gameServiceBound = false;
    private boolean gameProcessRunning = false;
    private String pendingSpecPath;
    private String pendingSessionId;

    private final ServiceConnection gameServiceConn = new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            Messenger m = new Messenger(service);
            String specToStart;
            String sessionToStart;
            Surface surf;
            synchronized (gameLock) {
                gameServiceMessenger = m;
                gameProcessRunning = true;
                specToStart = pendingSpecPath;
                sessionToStart = pendingSessionId;
                pendingSpecPath = null;
                pendingSessionId = null;
                surf = activeSurface;
            }
            if (specToStart != null && !specToStart.isEmpty()) {
                sendStartJvmMessage(m, specToStart, sessionToStart, surf);
            } else if (surf != null && surf.isValid()) {
                sendSurfaceUpdateMessage(m, surf);
            }
            writePermissionsSnapshot(bridgeDir());
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            synchronized (gameLock) {
                gameServiceMessenger = null;
                gameProcessRunning = false;
            }
            writePermissionsSnapshot(bridgeDir());
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        exportHardwareEnvironment();
        exportBridgeEnvironment();
        acquireMulticastLock();
        writeNetworkSnapshot(bridgeDir());
        configureDisplayCutout();
        super.onCreate(savedInstanceState);
        getWindow().setFormat(PixelFormat.TRANSLUCENT);
        applyImmersiveFullscreen();
        installWindowInsetsObserver();
        attachGameSurfaceView();
        startRuntimeExtractorThread();
        applyOrientationMode(loadSavedOrientationMode());
        refreshSafeInsetsFromWindow(null, false);
        writePermissionsSnapshot(bridgeDir());
        startBridgeWatcher();
    }

    @Override
    protected void onResume() {
        super.onResume();
        appInForeground = true;
        notifyKeepAliveForegroundState(true);
        applyImmersiveFullscreen();
        refreshSafeInsetsFromWindow(null, false);
        writePermissionsSnapshot(bridgeDir());
    }

    @Override
    protected void onPause() {
        appInForeground = false;
        notifyKeepAliveForegroundState(false);
        super.onPause();
    }

    private void notifyKeepAliveForegroundState(boolean foreground) {
        if (!ThkKeepAliveService.isServiceRunning()) {
            return;
        }
        try {
            Intent intent = new Intent(this, ThkKeepAliveService.class);
            intent.setAction(ThkKeepAliveService.ACTION_START_OR_UPDATE);
            intent.putExtra(ThkKeepAliveService.EXTRA_APP_FOREGROUND, foreground);
            startService(intent);
        } catch (Throwable ignored) {
        }
    }

    private void startRuntimeExtractorThread() {
        Context appCtx = getApplicationContext();
        Thread unpack = new Thread(() -> {
            try {
                MobileRuntimeInstaller.ensureExtracted(appCtx);
            } catch (Throwable ignored) {
            }
        }, "thk-runtime-unpack");
        unpack.setDaemon(true);
        unpack.start();
    }

    private void attachGameSurfaceView() {
        SurfaceView sv = new SurfaceView(this) {
            @Override
            public boolean gatherTransparentRegion(Region region) {
                if (region != null) {
                    region.setEmpty();
                }
                return true;
            }
        };
        sv.setFocusable(false);
        sv.setFocusableInTouchMode(false);
        sv.setClickable(false);
        SurfaceHolder holder = sv.getHolder();
        holder.setFormat(PixelFormat.RGBA_8888);
        holder.addCallback(new SurfaceHolder.Callback() {
            @Override
            public void surfaceCreated(SurfaceHolder h) {
                Rect frame = h.getSurfaceFrame();
                int w = (frame != null && frame.width() > 0) ? frame.width() : sv.getWidth();
                int ht = (frame != null && frame.height() > 0) ? frame.height() : sv.getHeight();
                Messenger target;
                Surface s = h.getSurface();
                synchronized (gameLock) {
                    activeSurface = s;
                    surfaceReady = s != null && s.isValid();
                    if (w > 0 && ht > 0) {
                        surfacePhysWidth = w;
                        surfacePhysHeight = ht;
                    }
                    target = gameServiceMessenger;
                }
                if (target != null && s != null && s.isValid()) {
                    sendSurfaceUpdateMessage(target, s);
                }
                refreshSafeInsetsFromWindow(null, false);
                writePermissionsSnapshot(bridgeDir());
            }

            @Override
            public void surfaceChanged(SurfaceHolder h, int format, int width, int height) {
                int physW = sv.getWidth() > 0 ? sv.getWidth() : width;
                int physH = sv.getHeight() > 0 ? sv.getHeight() : height;
                Messenger target;
                Surface s = h.getSurface();
                synchronized (gameLock) {
                    activeSurface = s;
                    surfaceReady = s != null && s.isValid();
                    if (physW > 0 && physH > 0) {
                        surfacePhysWidth = physW;
                        surfacePhysHeight = physH;
                    }
                    target = gameServiceMessenger;
                }
                if (target != null && s != null && s.isValid()) {
                    sendSurfaceUpdateMessage(target, s);
                }
                refreshSafeInsetsFromWindow(null, false);
                writePermissionsSnapshot(bridgeDir());
            }

            @Override
            public void surfaceDestroyed(SurfaceHolder h) {
                synchronized (gameLock) {
                    activeSurface = null;
                    surfaceReady = false;
                }
                writePermissionsSnapshot(bridgeDir());
            }
        });
        addContentView(
            sv,
            new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT
            )
        );
        gameSurfaceView = sv;
    }

    private static void sendStartJvmMessage(
        Messenger target,
        String specPath,
        String sessionId,
        Surface surface
    ) {
        try {
            Message msg = Message.obtain(null, GameJvmService.MSG_START_JVM);
            Bundle b = new Bundle();
            b.putString("spec_path", specPath);
            b.putString("session_id", sessionId != null ? sessionId : "");
            if (surface != null && surface.isValid()) {
                b.putParcelable("surface", surface);
            }
            msg.setData(b);
            target.send(msg);
        } catch (Throwable ignored) {
        }
    }

    private static void sendSurfaceUpdateMessage(Messenger target, Surface surface) {
        try {
            Message msg = Message.obtain(null, GameJvmService.MSG_UPDATE_SURFACE);
            Bundle b = new Bundle();
            if (surface != null && surface.isValid()) {
                b.putParcelable("surface", surface);
            }
            msg.setData(b);
            target.send(msg);
        } catch (Throwable ignored) {
        }
    }

    private void configureDisplayCutout() {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                WindowManager.LayoutParams lp = getWindow().getAttributes();
                lp.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                getWindow().setAttributes(lp);
            }
        } catch (Throwable ignored) {
        }
    }

    private static void exportHardwareEnvironment() {
        try {
            String manufacturer = "";
            String model = "";
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                if (Build.SOC_MANUFACTURER != null && !Build.SOC_MANUFACTURER.equalsIgnoreCase(Build.UNKNOWN)) {
                    manufacturer = Build.SOC_MANUFACTURER.trim();
                }
                if (Build.SOC_MODEL != null && !Build.SOC_MODEL.equalsIgnoreCase(Build.UNKNOWN)) {
                    model = Build.SOC_MODEL.trim();
                }
            }
            String combined = (manufacturer + " " + model).trim();
            if (combined.isEmpty() && Build.HARDWARE != null && !Build.HARDWARE.equalsIgnoreCase(Build.UNKNOWN)) {
                combined = Build.HARDWARE.trim();
            }
            if (!combined.isEmpty()) {
                Os.setenv("THK_ANDROID_SOC", combined, true);
            }
        } catch (Throwable ignored) {
        }
    }

    private void exportBridgeEnvironment() {
        try {
            Os.setenv("THK_ANDROID_BRIDGE_DIR", bridgeDir().getAbsolutePath(), true);
        } catch (Throwable ignored) {
        }
    }

    private void acquireMulticastLock() {
        try {
            WifiManager wm = (WifiManager) getApplicationContext().getSystemService(Context.WIFI_SERVICE);
            if (wm != null) {
                multicastLock = wm.createMulticastLock("thk-mc-lan-multicast");
                multicastLock.setReferenceCounted(false);
                multicastLock.acquire();
            }
        } catch (Throwable ignored) {
        }
    }

    private void releaseMulticastLock() {
        try {
            if (multicastLock != null && multicastLock.isHeld()) {
                multicastLock.release();
            }
        } catch (Throwable ignored) {
        }
    }

    private void installWindowInsetsObserver() {
        try {
            View decor = getWindow() != null ? getWindow().getDecorView() : null;
            if (decor == null) {
                return;
            }
            decor.setOnApplyWindowInsetsListener((v, windowInsets) -> {
                refreshSafeInsetsFromWindow(windowInsets, true);
                return v.onApplyWindowInsets(windowInsets);
            });
            decor.requestApplyInsets();
            decor.post(() -> refreshSafeInsetsFromWindow(null, true));
        } catch (Throwable ignored) {
        }
    }

    @SuppressWarnings("deprecation")
    private void refreshSafeInsetsFromWindow(WindowInsets supplied, boolean writeIfChanged) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            return;
        }
        try {
            View decor = getWindow() != null ? getWindow().getDecorView() : null;
            WindowInsets insets = supplied != null
                ? supplied
                : (decor != null ? decor.getRootWindowInsets() : null);
            int left = 0;
            int top = 0;
            int right = 0;
            int bottom = 0;
            if (insets != null) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    DisplayCutout cutout = insets.getDisplayCutout();
                    if (cutout != null) {
                        left = Math.max(left, cutout.getSafeInsetLeft());
                        top = Math.max(top, cutout.getSafeInsetTop());
                        right = Math.max(right, cutout.getSafeInsetRight());
                        bottom = Math.max(bottom, cutout.getSafeInsetBottom());
                    }
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    Insets vis = insets.getInsets(
                        WindowInsets.Type.displayCutout()
                            | WindowInsets.Type.systemBars()
                            | WindowInsets.Type.mandatorySystemGestures()
                    );
                    left = Math.max(left, vis.left);
                    top = Math.max(top, vis.top);
                    right = Math.max(right, vis.right);
                    bottom = Math.max(bottom, vis.bottom);
                } else {
                    left = Math.max(left, insets.getSystemWindowInsetLeft());
                    top = Math.max(top, insets.getSystemWindowInsetTop());
                    right = Math.max(right, insets.getSystemWindowInsetRight());
                    bottom = Math.max(bottom, insets.getSystemWindowInsetBottom());
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    boolean isLandscape = decor != null
                        && decor.getWidth() > decor.getHeight()
                        && decor.getHeight() > 0;
                    RoundedCorner tl = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_LEFT);
                    if (tl != null && tl.getRadius() > 0) {
                        int clear = Math.min(64, (int) Math.ceil(tl.getRadius() * 0.36));
                        if (isLandscape) {
                            left = Math.max(left, clear);
                        }
                        top = Math.max(top, isLandscape ? Math.min(36, clear) : clear);
                    }
                    RoundedCorner tr = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_RIGHT);
                    if (tr != null && tr.getRadius() > 0) {
                        int clear = Math.min(64, (int) Math.ceil(tr.getRadius() * 0.36));
                        if (isLandscape) {
                            right = Math.max(right, clear);
                        }
                        top = Math.max(top, isLandscape ? Math.min(36, clear) : clear);
                    }
                    RoundedCorner bl = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_LEFT);
                    if (bl != null && bl.getRadius() > 0) {
                        int clear = Math.min(64, (int) Math.ceil(bl.getRadius() * 0.38));
                        if (isLandscape) {
                            left = Math.max(left, clear);
                        }
                        bottom = Math.max(bottom, isLandscape ? Math.min(36, clear) : clear);
                    }
                    RoundedCorner br = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_RIGHT);
                    if (br != null && br.getRadius() > 0) {
                        int clear = Math.min(64, (int) Math.ceil(br.getRadius() * 0.38));
                        if (isLandscape) {
                            right = Math.max(right, clear);
                        }
                        bottom = Math.max(bottom, isLandscape ? Math.min(36, clear) : clear);
                    }
                }
            }
            int minDim = (decor != null && decor.getWidth() > 0 && decor.getHeight() > 0)
                ? Math.min(decor.getWidth(), decor.getHeight())
                : 0;
            int cap = minDim > 0 ? Math.max(48, minDim / 3) : 240;
            left = Math.max(0, Math.min(left, cap));
            top = Math.max(0, Math.min(top, cap));
            right = Math.max(0, Math.min(right, cap));
            bottom = Math.max(0, Math.min(bottom, cap));

            boolean changed = left != safeInsetLeftPx
                || top != safeInsetTopPx
                || right != safeInsetRightPx
                || bottom != safeInsetBottomPx;
            safeInsetLeftPx = left;
            safeInsetTopPx = top;
            safeInsetRightPx = right;
            safeInsetBottomPx = bottom;
            if (changed && writeIfChanged) {
                writePermissionsSnapshot(bridgeDir());
            }
        } catch (Throwable ignored) {
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            applyImmersiveFullscreen();
            refreshSafeInsetsFromWindow(null, false);
            writePermissionsSnapshot(bridgeDir());
        }
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        applyImmersiveFullscreen();
        refreshSafeInsetsFromWindow(null, true);
        View decor = getWindow() != null ? getWindow().getDecorView() : null;
        if (decor != null) {
            decor.requestApplyInsets();
            decor.post(() -> refreshSafeInsetsFromWindow(null, true));
        }
    }

    @Override
    protected void onDestroy() {
        bridgeRunning = false;
        if (bridgeThread != null) {
            bridgeThread.interrupt();
        }
        stopMobileGameSession();
        releaseMulticastLock();
        super.onDestroy();
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == REQ_POST_NOTIFICATIONS) {
            writePermissionsSnapshot(bridgeDir());
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQ_PICK_DOCUMENT) {
            return;
        }
        final String reqId;
        synchronized (pickLock) {
            reqId = activePickId;
            activePickId = null;
        }
        if (reqId == null) {
            return;
        }
        final int finalResult = resultCode;
        final Intent finalData = data;
        Thread copyWorker = new Thread(() -> handlePickedDocuments(reqId, finalResult, finalData), "thk-saf-copy");
        copyWorker.setDaemon(true);
        copyWorker.start();
    }

    @SuppressWarnings("deprecation")
    private void applyImmersiveFullscreen() {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                getWindow().setDecorFitsSystemWindows(false);
                WindowInsetsController controller = getWindow().getInsetsController();
                if (controller != null) {
                    controller.hide(WindowInsets.Type.statusBars() | WindowInsets.Type.navigationBars());
                    controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                    );
                }
            } else {
                View decor = getWindow().getDecorView();
                if (decor != null) {
                    decor.setSystemUiVisibility(
                        View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                    );
                }
            }
        } catch (Throwable ignored) {
        }
    }

    private File bridgeDir() {
        File dir = new File(getFilesDir(), ".thk_bridge");
        if (!dir.exists()) {
            //noinspection ResultOfMethodCallIgnored
            dir.mkdirs();
        }
        return dir;
    }

    private void startBridgeWatcher() {
        bridgeRunning = true;
        bridgeThread = new Thread(this::bridgeLoop, "thk-android-bridge");
        bridgeThread.setDaemon(true);
        bridgeThread.start();
    }

    private void bridgeLoop() {
        File dir = bridgeDir();
        int netTick = 0;
        int permTick = 0;
        while (bridgeRunning) {
            try {
                pollOrientation(dir);
                pollOpenUrl(dir);
                pollClipboard(dir);
                pollDocumentPick(dir);
                pollCommands(dir);
                permTick++;
                if (permTick >= 10) {
                    permTick = 0;
                    writePermissionsSnapshot(dir);
                }
                netTick++;
                if (netTick >= 30) {
                    netTick = 0;
                    writeNetworkSnapshot(dir);
                }
                Thread.sleep(80L);
            } catch (InterruptedException ignored) {
                break;
            } catch (Throwable ignored) {
            }
        }
    }

    private void pollCommands(File dir) {
        File[] cmdFiles = dir.listFiles((d, name) -> name.startsWith("cmd_") && name.endsWith(".json"));
        if (cmdFiles == null || cmdFiles.length == 0) {
            return;
        }
        Arrays.sort(cmdFiles, (a, b) -> Long.compare(a.lastModified(), b.lastModified()));
        for (File cmdFile : cmdFiles) {
            String raw = readUtf8AndDelete(cmdFile);
            if (raw == null || raw.trim().isEmpty()) {
                continue;
            }
            try {
                JSONObject cmd = new JSONObject(raw);
                String op = cmd.optString("op", "").trim();
                handleBridgeCommand(op, cmd);
            } catch (Throwable ignored) {
            }
        }
        writePermissionsSnapshot(dir);
    }

    private void handleBridgeCommand(String op, JSONObject cmd) {
        switch (op) {
            case "ensure_mobile_runtime":
                startRuntimeExtractorThread();
                break;
            case "request_ignore_battery":
                runOnUiThread(this::requestIgnoreBatteryOptimizations);
                break;
            case "request_overlay_permission":
                runOnUiThread(this::requestOverlayPermission);
                break;
            case "request_notification_permission":
                runOnUiThread(this::requestNotificationPermission);
                break;
            case "set_keep_alive": {
                boolean enabled = cmd.optBoolean("enabled", false);
                boolean overlay = cmd.optBoolean("overlay", false);
                String status = cmd.optString("status", "");
                runOnUiThread(() -> updateKeepAliveService(enabled, overlay, status));
                break;
            }
            case "launch_mobile_game": {
                String sessionId = cmd.optString("session_id", "");
                String specPath = cmd.optString("spec_path", "");
                int winW = cmd.optInt("window_width", 0);
                int winH = cmd.optInt("window_height", 0);
                if (!specPath.isEmpty()) {
                    runOnUiThread(() -> startMobileGameSession(sessionId, specPath, winW, winH));
                }
                break;
            }
            case "stop_mobile_game":
                runOnUiThread(this::stopMobileGameSession);
                break;
            default:
                break;
        }
    }

    private void requestIgnoreBatteryOptimizations() {
        try {
            PowerManager pm = (PowerManager) getSystemService(Context.POWER_SERVICE);
            if (pm != null && !pm.isIgnoringBatteryOptimizations(getPackageName())) {
                Intent intent = new Intent(
                    Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                    Uri.parse("package:" + getPackageName())
                );
                startActivity(intent);
                return;
            }
            Intent fallback = new Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS);
            startActivity(fallback);
        } catch (Throwable ignored) {
            try {
                Intent appSettings = new Intent(
                    Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                    Uri.parse("package:" + getPackageName())
                );
                startActivity(appSettings);
            } catch (Throwable ignoredInner) {
            }
        }
    }

    private void requestOverlayPermission() {
        try {
            Intent intent = new Intent(
                Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                Uri.parse("package:" + getPackageName())
            );
            startActivity(intent);
        } catch (Throwable ignored) {
        }
    }

    private void requestNotificationPermission() {
        try {
            if (Build.VERSION.SDK_INT >= 33) {
                if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                    != PackageManager.PERMISSION_GRANTED) {
                    requestPermissions(
                        new String[]{Manifest.permission.POST_NOTIFICATIONS},
                        REQ_POST_NOTIFICATIONS
                    );
                    return;
                }
            }
            Intent intent = new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, getPackageName());
            startActivity(intent);
        } catch (Throwable ignored) {
        }
    }

    private void updateKeepAliveService(boolean enabled, boolean overlay, String statusText) {
        try {
            Intent intent = new Intent(this, ThkKeepAliveService.class);
            if (enabled) {
                intent.setAction(ThkKeepAliveService.ACTION_START_OR_UPDATE);
                intent.putExtra(ThkKeepAliveService.EXTRA_SHOW_OVERLAY, overlay);
                intent.putExtra(ThkKeepAliveService.EXTRA_APP_FOREGROUND, appInForeground);
                if (statusText != null && !statusText.isEmpty()) {
                    intent.putExtra(ThkKeepAliveService.EXTRA_STATUS_TEXT, statusText);
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    startForegroundService(intent);
                } else {
                    startService(intent);
                }
            } else if (ThkKeepAliveService.isServiceRunning()) {
                intent.setAction(ThkKeepAliveService.ACTION_STOP);
                startService(intent);
            }
        } catch (Throwable ignored) {
        }
    }

    private void startMobileGameSession(String sessionId, String specPath, int winW, int winH) {
        try {
            if (gameServiceBound) {
                stopMobileGameSession();
            }
            if (gameSurfaceView != null && winW >= 320 && winH >= 240) {
                gameSurfaceView.getHolder().setFixedSize(winW, winH);
            }
            synchronized (gameLock) {
                pendingSessionId = sessionId;
                pendingSpecPath = specPath;
            }
            Intent svcIntent = new Intent(this, GameJvmService.class);
            startService(svcIntent);
            gameServiceBound = bindService(
                svcIntent,
                gameServiceConn,
                Context.BIND_AUTO_CREATE | Context.BIND_IMPORTANT
            );
            writePermissionsSnapshot(bridgeDir());
        } catch (Throwable ignored) {
        }
    }

    private void stopMobileGameSession() {
        Messenger target;
        synchronized (gameLock) {
            target = gameServiceMessenger;
            gameServiceMessenger = null;
            gameProcessRunning = false;
            pendingSpecPath = null;
            pendingSessionId = null;
        }
        if (target != null) {
            try {
                target.send(Message.obtain(null, GameJvmService.MSG_STOP_JVM));
            } catch (Throwable ignored) {
            }
        }
        if (gameServiceBound) {
            gameServiceBound = false;
            try {
                unbindService(gameServiceConn);
            } catch (Throwable ignored) {
            }
        }
        try {
            stopService(new Intent(this, GameJvmService.class));
        } catch (Throwable ignored) {
        }
        if (gameSurfaceView != null) {
            try {
                gameSurfaceView.getHolder().setSizeFromLayout();
            } catch (Throwable ignored) {
            }
        }
        writePermissionsSnapshot(bridgeDir());
    }

    private void writePermissionsSnapshot(File dir) {
        try {
            if (Looper.myLooper() == Looper.getMainLooper()) {
                refreshSafeInsetsFromWindow(null, false);
            }
            boolean ignoreBattery = false;
            PowerManager pm = (PowerManager) getSystemService(Context.POWER_SERVICE);
            if (pm != null) {
                ignoreBattery = pm.isIgnoringBatteryOptimizations(getPackageName());
            }

            boolean canOverlay = Settings.canDrawOverlays(this);

            boolean notifEnabled = true;
            NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
            if (nm != null) {
                notifEnabled = nm.areNotificationsEnabled();
            }

            boolean surfReadySnap;
            int surfWSnap;
            int surfHSnap;
            boolean gameRunningSnap;
            synchronized (gameLock) {
                surfReadySnap = surfaceReady;
                surfWSnap = surfacePhysWidth;
                surfHSnap = surfacePhysHeight;
                gameRunningSnap = gameProcessRunning;
            }
            if ((surfWSnap <= 0 || surfHSnap <= 0) && gameSurfaceView != null) {
                int vw = gameSurfaceView.getWidth();
                int vh = gameSurfaceView.getHeight();
                if (vw > 0 && vh > 0) {
                    surfWSnap = vw;
                    surfHSnap = vh;
                }
            }

            JSONObject obj = new JSONObject();
            obj.put("ignore_battery_optimizations", ignoreBattery);
            obj.put("can_draw_overlays", canOverlay);
            obj.put("notifications_enabled", notifEnabled);
            obj.put("keep_alive_running", ThkKeepAliveService.isServiceRunning());
            obj.put("overlay_showing", ThkKeepAliveService.isOverlayShowing());
            obj.put("surface_ready", surfReadySnap);
            obj.put("surface_width", Math.max(0, surfWSnap));
            obj.put("surface_height", Math.max(0, surfHSnap));
            obj.put("game_process_running", gameRunningSnap);
            obj.put("safe_inset_left_px", Math.max(0, safeInsetLeftPx));
            obj.put("safe_inset_top_px", Math.max(0, safeInsetTopPx));
            obj.put("safe_inset_right_px", Math.max(0, safeInsetRightPx));
            obj.put("safe_inset_bottom_px", Math.max(0, safeInsetBottomPx));

            byte[] bytes = obj.toString().getBytes(StandardCharsets.UTF_8);
            File tmp = new File(dir, "permissions.tmp");
            File dst = new File(dir, "permissions.json");
            try (FileOutputStream out = new FileOutputStream(tmp)) {
                out.write(bytes);
                out.flush();
            }
            if (dst.exists()) {
                dst.delete();
            }
            tmp.renameTo(dst);
        } catch (Throwable ignored) {
        }
    }

    private void writeNetworkSnapshot(File dir) {
        try {
            Map<String, String> v4ByIp = new LinkedHashMap<>();
            List<String> globalV6 = new ArrayList<>();

            ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
            if (cm != null) {
                Network active = cm.getActiveNetwork();
                if (active != null) {
                    collectLinkProperties(cm.getLinkProperties(active), v4ByIp, globalV6);
                }
                Network[] all = cm.getAllNetworks();
                if (all != null) {
                    for (Network net : all) {
                        if (net != null && !net.equals(active)) {
                            collectLinkProperties(cm.getLinkProperties(net), v4ByIp, globalV6);
                        }
                    }
                }
            }

            try {
                Enumeration<NetworkInterface> nics = NetworkInterface.getNetworkInterfaces();
                if (nics != null) {
                    while (nics.hasMoreElements()) {
                        NetworkInterface nic = nics.nextElement();
                        if (nic == null || !nic.isUp() || nic.isLoopback()) {
                            continue;
                        }
                        String name = nic.getName() != null ? nic.getName() : "net";
                        Enumeration<InetAddress> addrs = nic.getInetAddresses();
                        while (addrs != null && addrs.hasMoreElements()) {
                            collectInetAddress(name, addrs.nextElement(), v4ByIp, globalV6);
                        }
                    }
                }
            } catch (Throwable ignored) {
            }

            JSONObject root = new JSONObject();
            JSONArray ifaces = new JSONArray();
            for (Map.Entry<String, String> entry : v4ByIp.entrySet()) {
                JSONObject item = new JSONObject();
                item.put("name", entry.getValue());
                item.put("ipv4", entry.getKey());
                ifaces.put(item);
            }
            root.put("interfaces", ifaces);
            JSONArray v6Arr = new JSONArray();
            for (String v6 : globalV6) {
                v6Arr.put(v6);
            }
            root.put("ipv6", v6Arr);

            byte[] bytes = root.toString().getBytes(StandardCharsets.UTF_8);
            File tmp = new File(dir, "networks.tmp");
            File dst = new File(dir, "networks.json");
            try (FileOutputStream out = new FileOutputStream(tmp)) {
                out.write(bytes);
                out.flush();
            }
            if (dst.exists()) {
                dst.delete();
            }
            tmp.renameTo(dst);
        } catch (Throwable ignored) {
        }
    }

    private static void collectLinkProperties(
        LinkProperties lp,
        Map<String, String> v4ByIp,
        List<String> globalV6
    ) {
        if (lp == null) {
            return;
        }
        String ifaceName = lp.getInterfaceName();
        if (ifaceName == null || ifaceName.trim().isEmpty()) {
            ifaceName = "net";
        }
        for (LinkAddress la : lp.getLinkAddresses()) {
            if (la != null) {
                collectInetAddress(ifaceName, la.getAddress(), v4ByIp, globalV6);
            }
        }
    }

    private static void collectInetAddress(
        String ifaceName,
        InetAddress addr,
        Map<String, String> v4ByIp,
        List<String> globalV6
    ) {
        if (addr == null || addr.isLoopbackAddress() || addr.isLinkLocalAddress() || addr.isMulticastAddress()) {
            return;
        }
        String raw = addr.getHostAddress();
        if (raw == null) {
            return;
        }
        int pct = raw.indexOf('%');
        String clean = (pct >= 0 ? raw.substring(0, pct) : raw).trim();
        if (clean.isEmpty()) {
            return;
        }
        if (addr instanceof Inet4Address) {
            if (!v4ByIp.containsKey(clean)) {
                v4ByIp.put(clean, ifaceName);
            }
        } else if (addr instanceof Inet6Address) {
            byte[] octets = addr.getAddress();
            if (octets != null && octets.length == 16 && ((octets[0] & 0xE0) == 0x20)) {
                if (!globalV6.contains(clean)) {
                    globalV6.add(clean);
                }
            }
        }
    }

    private String loadSavedOrientationMode() {
        File saved = new File(bridgeDir(), "saved_orientation.txt");
        if (!saved.isFile()) {
            return "full_sensor";
        }
        try {
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            byte[] buf = new byte[256];
            try (FileInputStream in = new FileInputStream(saved)) {
                int n;
                while ((n = in.read(buf)) != -1) {
                    bos.write(buf, 0, n);
                }
            }
            String mode = bos.toString(StandardCharsets.UTF_8.name()).trim();
            return mode.isEmpty() ? "full_sensor" : mode;
        } catch (Throwable ignored) {
            return "full_sensor";
        }
    }

    private void pollOrientation(File dir) {
        File orientFile = new File(dir, "orientation.txt");
        if (!orientFile.isFile()) {
            return;
        }
        String raw = readUtf8AndDelete(orientFile);
        if (raw == null) {
            return;
        }
        final String mode = raw.trim();
        if (mode.isEmpty()) {
            return;
        }
        runOnUiThread(() -> {
            applyOrientationMode(mode);
            applyImmersiveFullscreen();
            refreshSafeInsetsFromWindow(null, true);
        });
    }

    private void applyOrientationMode(String mode) {
        try {
            int target;
            switch (mode.toLowerCase()) {
                case "unspecified":
                case "auto":
                    target = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED;
                    break;
                case "sensor_landscape":
                    target = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE;
                    break;
                case "landscape":
                    target = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE;
                    break;
                case "reverse_landscape":
                    target = ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE;
                    break;
                case "portrait":
                    target = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT;
                    break;
                case "full_sensor":
                default:
                    target = ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR;
                    break;
            }
            if (getRequestedOrientation() != target) {
                setRequestedOrientation(target);
                View decor = getWindow() != null ? getWindow().getDecorView() : null;
                if (decor != null) {
                    decor.post(() -> refreshSafeInsetsFromWindow(null, true));
                }
            }
        } catch (Throwable ignored) {
        }
    }

    private void pollOpenUrl(File dir) {
        File urlFile = new File(dir, "open_url.txt");
        if (!urlFile.isFile()) {
            return;
        }
        String url = readUtf8AndDelete(urlFile);
        if (url == null) {
            return;
        }
        String trimmed = url.trim();
        if (!trimmed.startsWith("http://") && !trimmed.startsWith("https://")) {
            return;
        }
        runOnUiThread(() -> {
            try {
                Intent viewIntent = new Intent(Intent.ACTION_VIEW, Uri.parse(trimmed));
                viewIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                startActivity(viewIntent);
            } catch (Throwable ignored) {
            }
        });
    }

    private void pollClipboard(File dir) {
        File clipFile = new File(dir, "clipboard.txt");
        if (!clipFile.isFile()) {
            return;
        }
        String text = readUtf8AndDelete(clipFile);
        if (text == null) {
            return;
        }
        runOnUiThread(() -> {
            try {
                ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                if (cm != null) {
                    cm.setPrimaryClip(ClipData.newPlainText("TriHienKun Laucher", text));
                }
            } catch (Throwable ignored) {
            }
        });
    }

    private void pollDocumentPick(File dir) {
        File reqFile = new File(dir, "pick_req.json");
        if (!reqFile.isFile()) {
            return;
        }
        synchronized (pickLock) {
            if (activePickId != null) {
                return;
            }
            String raw = readUtf8AndDelete(reqFile);
            if (raw == null || raw.isEmpty()) {
                return;
            }
            try {
                JSONObject obj = new JSONObject(raw);
                String reqId = obj.optString("id", "").trim();
                if (reqId.isEmpty()) {
                    return;
                }
                boolean multiple = obj.optBoolean("multiple", false);
                JSONArray exts = obj.optJSONArray("extensions");
                String mimeType = "*/*";
                if (exts != null && exts.length() == 1 && "png".equalsIgnoreCase(exts.optString(0))) {
                    mimeType = "image/png";
                }
                activePickId = reqId;
                final String resolvedMime = mimeType;
                runOnUiThread(() -> {
                    try {
                        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                        intent.addCategory(Intent.CATEGORY_OPENABLE);
                        intent.setType(resolvedMime);
                        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
                        startActivityForResult(intent, REQ_PICK_DOCUMENT);
                    } catch (Throwable err) {
                        synchronized (pickLock) {
                            activePickId = null;
                        }
                        writePickResponse(reqId, Collections.emptyList());
                    }
                });
            } catch (Throwable ignored) {
            }
        }
    }

    private void handlePickedDocuments(String reqId, int resultCode, Intent data) {
        if (resultCode != Activity.RESULT_OK || data == null) {
            writePickResponse(reqId, Collections.emptyList());
            return;
        }
        List<Uri> uris = new ArrayList<>();
        ClipData clip = data.getClipData();
        if (clip != null) {
            for (int i = 0; i < clip.getItemCount(); i++) {
                Uri itemUri = clip.getItemAt(i).getUri();
                if (itemUri != null) {
                    uris.add(itemUri);
                }
            }
        } else if (data.getData() != null) {
            uris.add(data.getData());
        }

        File pickedDir = new File(getFilesDir(), ".TriHienKunLaucher/cache/picked");
        if (!pickedDir.exists()) {
            //noinspection ResultOfMethodCallIgnored
            pickedDir.mkdirs();
        }

        List<String> stagedPaths = new ArrayList<>();
        byte[] buffer = new byte[65536];
        for (Uri uri : uris) {
            String displayName = sanitizeFileName(queryDisplayName(uri));
            File dest = new File(pickedDir, UUID.randomUUID().toString().substring(0, 8) + "_" + displayName);
            try (InputStream in = getContentResolver().openInputStream(uri);
                 FileOutputStream out = new FileOutputStream(dest)) {
                if (in == null) {
                    continue;
                }
                int read;
                while ((read = in.read(buffer)) != -1) {
                    out.write(buffer, 0, read);
                }
                out.flush();
                stagedPaths.add(dest.getAbsolutePath());
            } catch (Throwable ignored) {
                //noinspection ResultOfMethodCallIgnored
                dest.delete();
            }
        }
        writePickResponse(reqId, stagedPaths);
    }

    private String queryDisplayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (idx >= 0) {
                    String name = cursor.getString(idx);
                    if (name != null && !name.trim().isEmpty()) {
                        return name.trim();
                    }
                }
            }
        } catch (Throwable ignored) {
        }
        String last = uri.getLastPathSegment();
        return (last != null && !last.isEmpty()) ? last : "picked_file.bin";
    }

    private static String sanitizeFileName(String raw) {
        String cleaned = raw.replaceAll("[\\\\/:*?\"<>|\\u0000]", "_").trim();
        if (cleaned.isEmpty() || ".".equals(cleaned) || "..".equals(cleaned)) {
            return "picked_file.bin";
        }
        return cleaned;
    }

    private void writePickResponse(String reqId, List<String> paths) {
        try {
            JSONObject resp = new JSONObject();
            JSONArray arr = new JSONArray();
            for (String p : paths) {
                arr.put(p);
            }
            resp.put("paths", arr);
            byte[] bytes = resp.toString().getBytes(StandardCharsets.UTF_8);
            File dir = bridgeDir();
            File tmp = new File(dir, "pick_resp_" + reqId + ".tmp");
            File dst = new File(dir, "pick_resp_" + reqId + ".json");
            try (FileOutputStream out = new FileOutputStream(tmp)) {
                out.write(bytes);
                out.flush();
            }
            if (dst.exists()) {
                dst.delete();
            }
            tmp.renameTo(dst);
        } catch (Throwable ignored) {
        }
    }

    private static String readUtf8AndDelete(File file) {
        try {
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            byte[] buf = new byte[4096];
            try (FileInputStream in = new FileInputStream(file)) {
                int n;
                while ((n = in.read(buf)) != -1) {
                    bos.write(buf, 0, n);
                }
            }
            //noinspection ResultOfMethodCallIgnored
            file.delete();
            return bos.toString(StandardCharsets.UTF_8.name());
        } catch (Throwable err) {
            //noinspection ResultOfMethodCallIgnored
            file.delete();
            return null;
        }
    }
}
