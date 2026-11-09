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
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Insets;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PixelFormat;
import android.graphics.Rect;
import android.graphics.RectF;
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
import android.os.ParcelFileDescriptor;
import android.os.PowerManager;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.util.DisplayMetrics;
import android.view.DisplayCutout;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.RoundedCorner;
import android.view.Surface;
import android.view.SurfaceControl;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.FrameLayout;

import java.net.Inet4Address;
import java.net.Inet6Address;
import java.net.InetAddress;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

public final class MainActivity extends NativeActivity {
    static {
        ThkNativeBridge.ensureLoaded();
    }

    private static final int REQ_PICK_DOCUMENT = 4101;
    private static final int REQ_POST_NOTIFICATIONS = 4102;

    private final Object pickLock = new Object();
    private final Object gameLock = new Object();

    private volatile boolean appInForeground = false;
    private String activePickId;
    private WifiManager.MulticastLock multicastLock;
    private PowerManager.WakeLock screenWakeLock;

    private FrameLayout gameSurfaceHost;
    private volatile SurfaceView gameSurfaceView;
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
    private boolean gameSessionActive = false;
    private boolean gameViewFront = false;
    private String pendingSpecPath;
    private String pendingSessionId;
    private InAppSwitchIconView inAppToggleOrb;

    private final ServiceConnection gameServiceConn = new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            Messenger m = new Messenger(service);
            String specToStart;
            String sessionToStart;
            Surface surf;
            int surfW;
            int surfH;
            synchronized (gameLock) {
                gameServiceMessenger = m;
                gameProcessRunning = true;
                specToStart = pendingSpecPath;
                sessionToStart = pendingSessionId;
                pendingSpecPath = null;
                pendingSessionId = null;
                surf = activeSurface;
                surfW = surfacePhysWidth;
                surfH = surfacePhysHeight;
            }
            SurfaceView sv = gameSurfaceView;
            if ((surf == null || !surf.isValid()) && sv != null) {
                Surface holderSurf = sv.getHolder().getSurface();
                if (holderSurf != null && holderSurf.isValid()) {
                    surf = holderSurf;
                }
                if (surfW <= 0 || surfH <= 0) {
                    surfW = sv.getWidth();
                    surfH = sv.getHeight();
                }
            }
            if (specToStart != null && !specToStart.isEmpty()) {
                sendStartJvmMessage(m, specToStart, sessionToStart, surf, surfW, surfH);
            } else if (surf != null && surf.isValid()) {
                sendSurfaceUpdateMessage(m, surf, surfW, surfH);
            }
            syncHostAndNetworkToRust(false);
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            synchronized (gameLock) {
                gameServiceMessenger = null;
                gameProcessRunning = false;
            }
            runOnUiThread(() -> {
                gameServiceBound = false;
                gameSessionActive = false;
                setGameSurfaceFront(false);
                removeInAppGameToggleOrb();
            });
            syncHostAndNetworkToRust(false);
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        ThkNativeBridge.nativeInitHost(
            this,
            getFilesDir().getAbsolutePath(),
            getApplicationInfo().sourceDir,
            resolveSocHint()
        );
        acquireMulticastLock();
        acquireScreenWakeLock();
        syncHostAndNetworkToRust(true);
        configureDisplayCutout();
        getWindow().setFormat(PixelFormat.TRANSLUCENT);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            setShowWhenLocked(true);
            setTurnScreenOn(true);
        }
        super.onCreate(savedInstanceState);
        promoteOverLockScreen();
        applyImmersiveFullscreen();
        installWindowInsetsObserver();
        attachGameSurfaceView();
        applyOrientationMode(ThkNativeBridge.nativeReadSavedOrientation(getFilesDir().getAbsolutePath()));
        refreshSafeInsetsFromWindow(null, false);
        syncHostAndNetworkToRust(false);
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        applyFocusIntentIfPresent(intent);
    }

    @SuppressWarnings("deprecation")
    private void promoteOverLockScreen() {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
                setShowWhenLocked(true);
                setTurnScreenOn(true);
            }
            getWindow().addFlags(
                WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED
                    | WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON
                    | WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
            );
        } catch (Throwable ignored) {
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        promoteOverLockScreen();
        acquireScreenWakeLock();
        appInForeground = true;
        notifyKeepAliveForegroundState(true);
        applyFocusIntentIfPresent(getIntent());
        applyImmersiveFullscreen();
        refreshSafeInsetsFromWindow(null, false);
        attachGameSurfaceView();
        if (gameSessionActive) {
            ensureInAppGameToggleOrb();
        }
        syncHostAndNetworkToRust(true);
    }

    private void applyFocusIntentIfPresent(Intent intent) {
        if (intent == null || !intent.hasExtra("thk_focus_mode")) {
            return;
        }
        String mode = intent.getStringExtra("thk_focus_mode");
        intent.removeExtra("thk_focus_mode");
        if (mode == null || !gameSessionActive) {
            return;
        }
        if ("hub".equalsIgnoreCase(mode)) {
            setGameSurfaceFront(false);
        } else if ("game".equalsIgnoreCase(mode)) {
            setGameSurfaceFront(true);
        }
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

    @SuppressWarnings("deprecation")
    private void attachGameSurfaceView() {
        if (isFinishing() || isDestroyed()) {
            return;
        }
        if (gameSurfaceHost != null && gameSurfaceView != null) {
            syncGameSurfaceVisibility();
            return;
        }
        WindowManager wm = getWindowManager();
        if (wm == null) {
            return;
        }

        FrameLayout host = new FrameLayout(this);
        host.setBackgroundColor(Color.TRANSPARENT);
        host.setFocusable(false);
        host.setClickable(false);

        SurfaceView sv = new SurfaceView(this);
        sv.setFocusable(false);
        sv.setFocusableInTouchMode(false);
        sv.setClickable(false);
        sv.setZOrderMediaOverlay(true);
        sv.setVisibility(View.VISIBLE);
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
                applySurfaceControlVisibility(sv, gameSessionActive && gameViewFront);
                if (target != null && s != null && s.isValid()) {
                    sendSurfaceUpdateMessage(target, s, w, ht);
                }
                refreshSafeInsetsFromWindow(null, false);
                syncHostAndNetworkToRust(false);
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
                applySurfaceControlVisibility(sv, gameSessionActive && gameViewFront);
                if (target != null && s != null && s.isValid()) {
                    sendSurfaceUpdateMessage(target, s, physW, physH);
                }
                refreshSafeInsetsFromWindow(null, false);
                syncHostAndNetworkToRust(false);
            }

            @Override
            public void surfaceDestroyed(SurfaceHolder h) {
                synchronized (gameLock) {
                    activeSurface = null;
                    surfaceReady = false;
                }
                syncHostAndNetworkToRust(false);
            }
        });

        host.addView(
            sv,
            new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT
            )
        );

        int flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
            | WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
            | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
            | WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
            | WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED
            | WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON
            | WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED;
        WindowManager.LayoutParams lp = new WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION,
            flags,
            PixelFormat.TRANSLUCENT
        );
        lp.gravity = Gravity.TOP | Gravity.START;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            lp.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        }
        lp.alpha = 1.0f;

        try {
            wm.addView(host, lp);
            gameSurfaceHost = host;
            gameSurfaceView = sv;
        } catch (Throwable ignored) {
            gameSurfaceHost = null;
            gameSurfaceView = null;
        }
    }

    private void removeGameSurfaceWindow() {
        if (gameSurfaceHost != null) {
            try {
                WindowManager wm = getWindowManager();
                if (wm != null) {
                    wm.removeViewImmediate(gameSurfaceHost);
                }
            } catch (Throwable ignored) {
            }
        }
        gameSurfaceHost = null;
        gameSurfaceView = null;
    }

    private static void applySurfaceControlVisibility(SurfaceView sv, boolean visible) {
        if (sv == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            return;
        }
        try {
            SurfaceControl sc = sv.getSurfaceControl();
            if (sc != null && sc.isValid()) {
                new SurfaceControl.Transaction()
                    .setAlpha(sc, visible ? 1.0f : 0.0f)
                    .setVisibility(sc, visible)
                    .apply();
            }
        } catch (Throwable ignored) {
        }
    }

    private void syncGameSurfaceVisibility() {
        boolean show = gameSessionActive && gameViewFront;
        applySurfaceControlVisibility(gameSurfaceView, show);
    }

    private static void sendStartJvmMessage(
        Messenger target,
        String specPath,
        String sessionId,
        Surface surface,
        int width,
        int height
    ) {
        try {
            Message msg = Message.obtain(null, GameJvmService.MSG_START_JVM);
            Bundle b = new Bundle();
            b.putString("spec_path", specPath);
            b.putString("session_id", sessionId != null ? sessionId : "");
            if (width > 0 && height > 0) {
                b.putInt("width", width);
                b.putInt("height", height);
            }
            if (surface != null && surface.isValid()) {
                b.putParcelable("surface", surface);
            }
            msg.setData(b);
            target.send(msg);
        } catch (Throwable ignored) {
        }
    }

    private static void sendSurfaceUpdateMessage(
        Messenger target,
        Surface surface,
        int width,
        int height
    ) {
        try {
            Message msg = Message.obtain(null, GameJvmService.MSG_UPDATE_SURFACE);
            Bundle b = new Bundle();
            if (width > 0 && height > 0) {
                b.putInt("width", width);
                b.putInt("height", height);
            }
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

    private static String resolveSocHint() {
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
            return combined;
        } catch (Throwable ignored) {
            return "";
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

    @SuppressWarnings("deprecation")
    private void acquireScreenWakeLock() {
        try {
            PowerManager pm = (PowerManager) getApplicationContext().getSystemService(Context.POWER_SERVICE);
            if (pm != null) {
                if (screenWakeLock == null) {
                    screenWakeLock = pm.newWakeLock(
                        PowerManager.SCREEN_BRIGHT_WAKE_LOCK | PowerManager.ACQUIRE_CAUSES_WAKEUP | PowerManager.ON_AFTER_RELEASE,
                        "thk:screen-wake"
                    );
                    screenWakeLock.setReferenceCounted(false);
                }
                screenWakeLock.acquire(15 * 60 * 1000L);
            }
        } catch (Throwable ignored) {
        }
    }

    private void releaseScreenWakeLock() {
        try {
            if (screenWakeLock != null && screenWakeLock.isHeld()) {
                screenWakeLock.release();
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
    private void refreshSafeInsetsFromWindow(WindowInsets supplied, boolean syncIfChanged) {
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
            if (changed && syncIfChanged) {
                syncHostAndNetworkToRust(false);
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
            attachGameSurfaceView();
            if (gameSessionActive) {
                ensureInAppGameToggleOrb();
            }
            syncHostAndNetworkToRust(false);
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
        ThkNativeBridge.nativeShutdownHost();
        stopMobileGameSession();
        removeGameSurfaceWindow();
        releaseMulticastLock();
        releaseScreenWakeLock();
        super.onDestroy();
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == REQ_POST_NOTIFICATIONS) {
            syncHostAndNetworkToRust(false);
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
        Thread fdWorker = new Thread(() -> handlePickedDocuments(reqId, finalResult, finalData), "thk-saf-fd");
        fdWorker.setDaemon(true);
        fdWorker.start();
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

    public void onBridgeApplyOrientation(String mode) {
        if (mode == null || mode.trim().isEmpty()) {
            return;
        }
        final String trimmed = mode.trim();
        runOnUiThread(() -> {
            applyOrientationMode(trimmed);
            applyImmersiveFullscreen();
            refreshSafeInsetsFromWindow(null, true);
        });
    }

    public void onBridgeOpenUrl(String url) {
        if (url == null) {
            return;
        }
        final String trimmed = url.trim();
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

    public void onBridgeSetClipboard(String text) {
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

    public void onBridgePickDocument(String reqId, boolean multiple, String mimeType) {
        if (reqId == null || reqId.trim().isEmpty()) {
            return;
        }
        final String trimmedId = reqId.trim();
        final String resolvedMime = (mimeType != null && !mimeType.trim().isEmpty())
            ? mimeType.trim()
            : "*/*";
        runOnUiThread(() -> {
            synchronized (pickLock) {
                if (activePickId != null) {
                    ThkNativeBridge.nativeCompletePickRequest(
                        getFilesDir().getAbsolutePath(),
                        trimmedId,
                        new String[0]
                    );
                    return;
                }
                activePickId = trimmedId;
            }
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
                ThkNativeBridge.nativeCompletePickRequest(
                    getFilesDir().getAbsolutePath(),
                    trimmedId,
                    new String[0]
                );
            }
        });
    }

    public void onBridgeRequestPermission(int kind) {
        runOnUiThread(() -> {
            switch (kind) {
                case 1:
                    requestIgnoreBatteryOptimizations();
                    break;
                case 2:
                    requestOverlayPermission();
                    break;
                case 3:
                    requestNotificationPermission();
                    break;
                default:
                    break;
            }
            syncHostAndNetworkToRust(false);
        });
    }

    public void onBridgeSetKeepAlive(boolean enabled, boolean overlay, String status) {
        runOnUiThread(() -> {
            updateKeepAliveService(enabled, overlay, status);
            syncHostAndNetworkToRust(false);
        });
    }

    public void onBridgeLaunchGame(String sessionId, String specPath, int width, int height) {
        if (specPath == null || specPath.trim().isEmpty()) {
            return;
        }
        final String safeSession = sessionId != null ? sessionId : "";
        final String safeSpec = specPath.trim();
        runOnUiThread(() -> startMobileGameSession(safeSession, safeSpec, width, height));
    }

    public void onBridgeStopGame() {
        runOnUiThread(this::stopMobileGameSession);
    }

    public void onBridgeRefreshHostState(boolean includeNetworks) {
        syncHostAndNetworkToRust(includeNetworks);
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
            String launchOrient = ThkNativeBridge.nativeResolveLaunchOrientation(
                getFilesDir().getAbsolutePath(),
                winW,
                winH
            );
            applyOrientationMode(launchOrient);
            promoteOverLockScreen();
            acquireScreenWakeLock();
            applyImmersiveFullscreen();
            gameSessionActive = true;
            attachGameSurfaceView();
            setGameSurfaceFront(true);
            ensureInAppGameToggleOrb();
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
            syncHostAndNetworkToRust(false);
        } catch (Throwable ignored) {
        }
    }

    private void stopMobileGameSession() {
        gameSessionActive = false;
        setGameSurfaceFront(false);
        removeInAppGameToggleOrb();
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
        if (!isFinishing() && !isDestroyed()) {
            applyOrientationMode(ThkNativeBridge.nativeReadSavedOrientation(getFilesDir().getAbsolutePath()));
            applyImmersiveFullscreen();
            syncHostAndNetworkToRust(false);
        }
    }

    private void setGameSurfaceFront(boolean front) {
        gameViewFront = front;
        syncGameSurfaceVisibility();
        if (inAppToggleOrb != null) {
            inAppToggleOrb.setShowingGame(front);
        }
    }

    private void ensureInAppGameToggleOrb() {
        if (!gameSessionActive || isFinishing() || isDestroyed()) {
            return;
        }
        if (inAppToggleOrb != null) {
            inAppToggleOrb.setShowingGame(gameViewFront);
            return;
        }
        WindowManager wm = getWindowManager();
        if (wm == null) {
            return;
        }
        DisplayMetrics dm = getResources().getDisplayMetrics();
        float density = dm.density > 0f ? dm.density : 2.5f;
        int orbSize = Math.max(1, Math.round(42f * density));

        InAppSwitchIconView orb = new InAppSwitchIconView(this, density);
        orb.setShowingGame(gameViewFront);

        @SuppressWarnings("deprecation")
        int orbFlags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
            | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
            | WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED
            | WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON;
        WindowManager.LayoutParams lp = new WindowManager.LayoutParams(
            orbSize,
            orbSize,
            WindowManager.LayoutParams.TYPE_APPLICATION,
            orbFlags,
            PixelFormat.TRANSLUCENT
        );
        lp.gravity = Gravity.TOP | Gravity.START;
        boolean isLandscape = dm.widthPixels > dm.heightPixels;
        int rightMargin = safeInsetRightPx + Math.round(12f * density);
        int topOffset = Math.round((isLandscape ? 10f : 58f) * density);
        lp.x = Math.max(Math.round(12f * density), dm.widthPixels - orbSize - rightMargin);
        lp.y = Math.max(Math.round(12f * density), safeInsetTopPx + topOffset);

        orb.setOnTouchListener(new View.OnTouchListener() {
            private int startX;
            private int startY;
            private float downRawX;
            private float downRawY;
            private boolean dragged;

            @Override
            public boolean onTouch(View v, MotionEvent event) {
                switch (event.getActionMasked()) {
                    case MotionEvent.ACTION_DOWN:
                        startX = lp.x;
                        startY = lp.y;
                        downRawX = event.getRawX();
                        downRawY = event.getRawY();
                        dragged = false;
                        return true;
                    case MotionEvent.ACTION_MOVE:
                        float dx = event.getRawX() - downRawX;
                        float dy = event.getRawY() - downRawY;
                        if (Math.hypot(dx, dy) > 6f * density) {
                            dragged = true;
                        }
                        if (dragged && inAppToggleOrb != null) {
                            DisplayMetrics curDm = getResources().getDisplayMetrics();
                            int maxW = Math.max(orbSize, curDm.widthPixels - orbSize);
                            int maxH = Math.max(orbSize, curDm.heightPixels - orbSize);
                            lp.x = Math.max(0, Math.min(maxW, startX + Math.round(dx)));
                            lp.y = Math.max(0, Math.min(maxH, startY + Math.round(dy)));
                            try {
                                wm.updateViewLayout(inAppToggleOrb, lp);
                            } catch (Throwable ignored) {
                            }
                        }
                        return true;
                    case MotionEvent.ACTION_UP:
                        if (!dragged && gameSessionActive) {
                            boolean nextFront = !gameViewFront;
                            setGameSurfaceFront(nextFront);
                            ThkNativeBridge.nativeWriteFocusMode(
                                getFilesDir().getAbsolutePath(),
                                nextFront ? "game" : "hub"
                            );
                        }
                        return true;
                    default:
                        return false;
                }
            }
        });

        try {
            wm.addView(orb, lp);
            inAppToggleOrb = orb;
        } catch (Throwable ignored) {
            inAppToggleOrb = null;
        }
    }

    private void removeInAppGameToggleOrb() {
        if (inAppToggleOrb != null) {
            try {
                WindowManager wm = getWindowManager();
                if (wm != null) {
                    wm.removeViewImmediate(inAppToggleOrb);
                }
            } catch (Throwable ignored) {
            }
        }
        inAppToggleOrb = null;
    }

    private static final class InAppSwitchIconView extends View {
        private final Paint bgPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint ringPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint glyphPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Path triPath = new Path();
        private final RectF cellRect = new RectF();
        private final float density;
        private boolean showingGame = true;

        InAppSwitchIconView(Context context, float density) {
            super(context);
            this.density = density;
            bgPaint.setStyle(Paint.Style.FILL);
            bgPaint.setColor(Color.argb(204, 12, 20, 26));

            ringPaint.setStyle(Paint.Style.STROKE);
            ringPaint.setStrokeWidth(Math.max(1.5f, 1.7f * density));
            ringPaint.setColor(Color.argb(232, 52, 211, 153));

            glyphPaint.setStyle(Paint.Style.FILL);
            glyphPaint.setColor(Color.argb(245, 226, 252, 240));
        }

        void setShowingGame(boolean showingGame) {
            if (this.showingGame != showingGame) {
                this.showingGame = showingGame;
                invalidate();
            }
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            float cx = getWidth() * 0.5f;
            float cy = getHeight() * 0.5f;
            float radius = Math.min(cx, cy) - ringPaint.getStrokeWidth() * 0.5f - 1.0f;
            if (radius <= 0f) {
                return;
            }
            canvas.drawCircle(cx, cy, radius, bgPaint);
            canvas.drawCircle(cx, cy, radius, ringPaint);

            if (showingGame) {
                float cell = 4.4f * density;
                float gap = 1.6f * density;
                float r = 1.2f * density;
                for (int row = 0; row < 2; row++) {
                    for (int col = 0; col < 2; col++) {
                        float left = cx + (col == 0 ? -(cell + gap) : gap);
                        float top = cy + (row == 0 ? -(cell + gap) : gap);
                        cellRect.set(left, top, left + cell, top + cell);
                        canvas.drawRoundRect(cellRect, r, r, glyphPaint);
                    }
                }
            } else {
                float halfH = 6.2f * density;
                float left = cx - 4.2f * density;
                float right = cx + 6.4f * density;
                triPath.reset();
                triPath.moveTo(left, cy - halfH);
                triPath.lineTo(right, cy);
                triPath.lineTo(left, cy + halfH);
                triPath.close();
                canvas.drawPath(triPath, glyphPaint);
            }
        }
    }

    @SuppressWarnings("deprecation")
    private void syncHostAndNetworkToRust(boolean includeNetworks) {
        try {
            if (Looper.myLooper() == Looper.getMainLooper()) {
                refreshSafeInsetsFromWindow(null, false);
            } else if (!isFinishing() && !isDestroyed()) {
                runOnUiThread(() -> refreshSafeInsetsFromWindow(null, true));
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
            SurfaceView sv = gameSurfaceView;
            if ((surfWSnap <= 0 || surfHSnap <= 0) && sv != null) {
                int vw = sv.getWidth();
                int vh = sv.getHeight();
                if (vw > 0 && vh > 0) {
                    surfWSnap = vw;
                    surfHSnap = vh;
                }
            }

            ThkNativeBridge.nativeSyncHostState(
                ignoreBattery,
                canOverlay,
                notifEnabled,
                ThkKeepAliveService.isServiceRunning(),
                ThkKeepAliveService.isOverlayShowing(),
                surfReadySnap,
                Math.max(0, surfWSnap),
                Math.max(0, surfHSnap),
                gameRunningSnap,
                Math.max(0, safeInsetLeftPx),
                Math.max(0, safeInsetTopPx),
                Math.max(0, safeInsetRightPx),
                Math.max(0, safeInsetBottomPx)
            );

            if (includeNetworks) {
                Map<String, String> v4ByIp = new LinkedHashMap<>();
                List<String> globalV6 = new ArrayList<>();
                List<String> dnsServers = new ArrayList<>();

                ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
                if (cm != null) {
                    Network active = cm.getActiveNetwork();
                    if (active != null) {
                        collectLinkProperties(cm.getLinkProperties(active), v4ByIp, globalV6, dnsServers);
                    }
                    Network[] all = cm.getAllNetworks();
                    if (all != null) {
                        for (Network net : all) {
                            if (net != null && !net.equals(active)) {
                                collectLinkProperties(cm.getLinkProperties(net), v4ByIp, globalV6, dnsServers);
                            }
                        }
                    }
                }

                String[] ifacePairs = new String[v4ByIp.size()];
                int idx = 0;
                for (Map.Entry<String, String> entry : v4ByIp.entrySet()) {
                    ifacePairs[idx++] = entry.getValue() + "=" + entry.getKey();
                }

                ThkNativeBridge.nativeSyncNetworkFromJava(
                    ifacePairs,
                    globalV6.toArray(new String[0]),
                    dnsServers.toArray(new String[0])
                );
            }
        } catch (Throwable ignored) {
        }
    }

    private static void collectLinkProperties(
        LinkProperties lp,
        Map<String, String> v4ByIp,
        List<String> globalV6,
        List<String> dnsServers
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
        for (InetAddress dns : lp.getDnsServers()) {
            if (dns == null || dns.isLoopbackAddress() || dns.isLinkLocalAddress()) {
                continue;
            }
            String raw = dns.getHostAddress();
            if (raw == null) {
                continue;
            }
            int pct = raw.indexOf('%');
            String clean = (pct >= 0 ? raw.substring(0, pct) : raw).trim();
            if (!clean.isEmpty() && !dnsServers.contains(clean)) {
                dnsServers.add(clean);
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

    private void applyOrientationMode(String mode) {
        try {
            String safeMode = mode != null ? mode.trim().toLowerCase(Locale.ROOT) : "full_sensor";
            int target;
            switch (safeMode) {
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

    private void handlePickedDocuments(String reqId, int resultCode, Intent data) {
        String filesDir = getFilesDir().getAbsolutePath();
        if (resultCode != Activity.RESULT_OK || data == null) {
            ThkNativeBridge.nativeCompletePickRequest(filesDir, reqId, new String[0]);
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

        List<String> stagedPaths = new ArrayList<>();
        for (Uri uri : uris) {
            String displayName = queryDisplayName(uri);
            try (ParcelFileDescriptor pfd = getContentResolver().openFileDescriptor(uri, "r")) {
                if (pfd == null) {
                    continue;
                }
                String staged = ThkNativeBridge.nativeStagePickedFd(
                    filesDir,
                    pfd.detachFd(),
                    displayName
                );
                if (staged != null && !staged.trim().isEmpty()) {
                    stagedPaths.add(staged.trim());
                }
            } catch (Throwable ignored) {
            }
        }
        String[] stagedArray = stagedPaths.toArray(new String[0]);
        ThkNativeBridge.nativeCompletePickRequest(filesDir, reqId, stagedArray);
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
}
