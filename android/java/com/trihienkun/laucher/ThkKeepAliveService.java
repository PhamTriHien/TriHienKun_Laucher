package com.trihienkun.laucher;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.graphics.Color;
import android.graphics.PixelFormat;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.IBinder;
import android.os.PowerManager;
import android.provider.Settings;
import android.util.DisplayMetrics;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;
import android.widget.TextView;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;

public final class ThkKeepAliveService extends Service {
    public static final String ACTION_START_OR_UPDATE = "com.trihienkun.laucher.KEEP_ALIVE_UPDATE";
    public static final String ACTION_STOP = "com.trihienkun.laucher.KEEP_ALIVE_STOP";
    public static final String ACTION_OPEN_HUB = "com.trihienkun.laucher.OPEN_HUB";
    public static final String ACTION_RESUME_GAME = "com.trihienkun.laucher.RESUME_GAME";

    public static final String EXTRA_SHOW_OVERLAY = "show_overlay";
    public static final String EXTRA_APP_FOREGROUND = "app_foreground";
    public static final String EXTRA_STATUS_TEXT = "status_text";

    private static final String CHANNEL_ID = "thk_keep_alive_channel";
    private static final int NOTIFICATION_ID = 7104;

    private static volatile boolean serviceRunning = false;
    private static volatile boolean overlayShowing = false;

    private PowerManager.WakeLock cpuWakeLock;
    private WifiManager.WifiLock wifiLock;
    private WifiManager.MulticastLock multicastLock;

    private WindowManager windowManager;
    private View overlayOrb;
    private WindowManager.LayoutParams overlayParams;
    private boolean wantOverlay = false;
    private boolean appInForeground = true;
    private String currentStatusText = "Giữ kết nối mạng ổn định khi tắt màn hình hoặc chạy ngầm";

    public static boolean isServiceRunning() {
        return serviceRunning;
    }

    public static boolean isOverlayShowing() {
        return overlayShowing;
    }

    @Override
    public void onCreate() {
        super.onCreate();
        serviceRunning = true;
        windowManager = (WindowManager) getSystemService(Context.WINDOW_SERVICE);
        createNotificationChannel();
        acquireLocks();
        promoteForeground(currentStatusText);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null) {
            String action = intent.getAction();
            if (ACTION_STOP.equals(action)) {
                removeOverlayOrb();
                releaseLocks();
                serviceRunning = false;
                stopForeground(STOP_FOREGROUND_REMOVE);
                stopSelf();
                return START_NOT_STICKY;
            }
            if (ACTION_OPEN_HUB.equals(action)) {
                writeFocusMode("hub");
                bringLauncherToFront();
                return START_STICKY;
            }
            if (ACTION_RESUME_GAME.equals(action)) {
                writeFocusMode("game");
                bringLauncherToFront();
                return START_STICKY;
            }

            if (intent.hasExtra(EXTRA_STATUS_TEXT)) {
                String txt = intent.getStringExtra(EXTRA_STATUS_TEXT);
                if (txt != null && !txt.trim().isEmpty()) {
                    currentStatusText = txt.trim();
                }
            }
            if (intent.hasExtra(EXTRA_SHOW_OVERLAY)) {
                wantOverlay = intent.getBooleanExtra(EXTRA_SHOW_OVERLAY, false);
            }
            if (intent.hasExtra(EXTRA_APP_FOREGROUND)) {
                appInForeground = intent.getBooleanExtra(EXTRA_APP_FOREGROUND, true);
            }

            acquireLocks();
            promoteForeground(currentStatusText);
            syncOverlayVisibility();
        }
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        removeOverlayOrb();
        releaseLocks();
        serviceRunning = false;
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    private void syncOverlayVisibility() {
        if (!appInForeground && wantOverlay && Settings.canDrawOverlays(this)) {
            ensureOverlayOrb();
        } else {
            removeOverlayOrb();
        }
    }

    @SuppressWarnings("deprecation")
    private void acquireLocks() {
        try {
            PowerManager pm = (PowerManager) getSystemService(Context.POWER_SERVICE);
            if (pm != null) {
                if (cpuWakeLock == null) {
                    cpuWakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "thk:keep-alive-cpu");
                    cpuWakeLock.setReferenceCounted(false);
                }
                if (!cpuWakeLock.isHeld()) {
                    cpuWakeLock.acquire();
                }
            }
        } catch (Throwable ignored) {
        }

        try {
            WifiManager wm = (WifiManager) getApplicationContext().getSystemService(Context.WIFI_SERVICE);
            if (wm != null) {
                if (wifiLock == null) {
                    int mode = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q
                        ? WifiManager.WIFI_MODE_FULL_LOW_LATENCY
                        : WifiManager.WIFI_MODE_FULL_HIGH_PERF;
                    wifiLock = wm.createWifiLock(mode, "thk:keep-alive-wifi");
                    wifiLock.setReferenceCounted(false);
                }
                if (!wifiLock.isHeld()) {
                    wifiLock.acquire();
                }

                if (multicastLock == null) {
                    multicastLock = wm.createMulticastLock("thk:keep-alive-multicast");
                    multicastLock.setReferenceCounted(false);
                }
                if (!multicastLock.isHeld()) {
                    multicastLock.acquire();
                }
            }
        } catch (Throwable ignored) {
        }
    }

    private void releaseLocks() {
        try {
            if (cpuWakeLock != null && cpuWakeLock.isHeld()) {
                cpuWakeLock.release();
            }
        } catch (Throwable ignored) {
        }
        try {
            if (wifiLock != null && wifiLock.isHeld()) {
                wifiLock.release();
            }
        } catch (Throwable ignored) {
        }
        try {
            if (multicastLock != null && multicastLock.isHeld()) {
                multicastLock.release();
            }
        } catch (Throwable ignored) {
        }
    }

    private void createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
            if (nm != null) {
                NotificationChannel ch = new NotificationChannel(
                    CHANNEL_ID,
                    "TriHienKun Giữ Mạng & Chạy Ngầm",
                    NotificationManager.IMPORTANCE_LOW
                );
                ch.setDescription("Giữ kết nối mạng Server, Port và Game khi tắt màn hình hoặc thu nhỏ");
                ch.setShowBadge(false);
                nm.createNotificationChannel(ch);
            }
        }
    }

    private void promoteForeground(String statusText) {
        int piFlags = PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE;

        Intent launchIntent = new Intent(this, MainActivity.class);
        launchIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent contentPending = PendingIntent.getActivity(this, 100, launchIntent, piFlags);

        Intent hubActionIntent = new Intent(this, ThkKeepAliveService.class).setAction(ACTION_OPEN_HUB);
        PendingIntent hubPending = PendingIntent.getService(this, 101, hubActionIntent, piFlags);

        Intent gameActionIntent = new Intent(this, ThkKeepAliveService.class).setAction(ACTION_RESUME_GAME);
        PendingIntent gamePending = PendingIntent.getService(this, 102, gameActionIntent, piFlags);

        Intent stopActionIntent = new Intent(this, ThkKeepAliveService.class).setAction(ACTION_STOP);
        PendingIntent stopPending = PendingIntent.getService(this, 103, stopActionIntent, piFlags);

        Notification.Builder builder = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
            ? new Notification.Builder(this, CHANNEL_ID)
            : new Notification.Builder(this);

        builder.setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("TriHienKun Laucher — Chạy ngầm giữ mạng")
            .setContentText(statusText)
            .setOngoing(true)
            .setContentIntent(contentPending)
            .addAction(new Notification.Action.Builder(null, "Mở HUB UI", hubPending).build())
            .addAction(new Notification.Action.Builder(null, "Màn hình Game", gamePending).build())
            .addAction(new Notification.Action.Builder(null, "Tắt chạy ngầm", stopPending).build());

        Notification notification = builder.build();
        try {
            if (Build.VERSION.SDK_INT >= 34) {
                startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
            } else {
                startForeground(NOTIFICATION_ID, notification);
            }
        } catch (Throwable ignored) {
            try {
                startForeground(NOTIFICATION_ID, notification);
            } catch (Throwable ignoredInner) {
            }
        }
    }

    private void ensureOverlayOrb() {
        if (overlayOrb != null || windowManager == null || !Settings.canDrawOverlays(this)) {
            return;
        }
        DisplayMetrics dm = getResources().getDisplayMetrics();
        float density = dm.density;
        int orbSize = dp(40, density);

        TextView orb = new TextView(this);
        orb.setText("THK");
        orb.setTextColor(Color.WHITE);
        orb.setTypeface(Typeface.DEFAULT_BOLD);
        orb.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11.5f);
        orb.setGravity(Gravity.CENTER);

        GradientDrawable orbBg = new GradientDrawable();
        orbBg.setShape(GradientDrawable.OVAL);
        orbBg.setColor(Color.argb(230, 16, 185, 129));
        orbBg.setStroke(dp(2, density), Color.argb(235, 110, 231, 183));
        orb.setBackground(orbBg);

        int overlayType = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
            ? WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
            : WindowManager.LayoutParams.TYPE_PHONE;

        overlayParams = new WindowManager.LayoutParams(
            orbSize,
            orbSize,
            overlayType,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT
        );
        overlayParams.gravity = Gravity.TOP | Gravity.START;
        overlayParams.x = dp(14, density);
        overlayParams.y = dp(96, density);

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
                        startX = overlayParams.x;
                        startY = overlayParams.y;
                        downRawX = event.getRawX();
                        downRawY = event.getRawY();
                        dragged = false;
                        return true;
                    case MotionEvent.ACTION_MOVE:
                        float dx = event.getRawX() - downRawX;
                        float dy = event.getRawY() - downRawY;
                        if (Math.hypot(dx, dy) > dp(6, density)) {
                            dragged = true;
                        }
                        if (dragged && overlayOrb != null) {
                            overlayParams.x = Math.max(0, startX + Math.round(dx));
                            overlayParams.y = Math.max(0, startY + Math.round(dy));
                            try {
                                windowManager.updateViewLayout(overlayOrb, overlayParams);
                            } catch (Throwable ignored) {
                            }
                        }
                        return true;
                    case MotionEvent.ACTION_UP:
                        if (!dragged) {
                            bringLauncherToFront();
                        }
                        return true;
                    default:
                        return false;
                }
            }
        });

        try {
            windowManager.addView(orb, overlayParams);
            overlayOrb = orb;
            overlayShowing = true;
        } catch (Throwable ignored) {
            overlayOrb = null;
            overlayShowing = false;
        }
    }

    private void removeOverlayOrb() {
        if (overlayOrb != null && windowManager != null) {
            try {
                windowManager.removeView(overlayOrb);
            } catch (Throwable ignored) {
            }
        }
        overlayOrb = null;
        overlayShowing = false;
    }

    private void writeFocusMode(String mode) {
        try {
            File dir = new File(getFilesDir(), ".thk_bridge");
            if (!dir.exists()) {
                dir.mkdirs();
            }
            File tmp = new File(dir, "focus_mode.tmp");
            File dst = new File(dir, "focus_mode.txt");
            try (FileOutputStream out = new FileOutputStream(tmp)) {
                out.write(mode.getBytes(StandardCharsets.UTF_8));
                out.flush();
            }
            if (dst.exists()) {
                dst.delete();
            }
            tmp.renameTo(dst);
        } catch (Throwable ignored) {
        }
    }

    private void bringLauncherToFront() {
        try {
            Intent intent = new Intent(this, MainActivity.class);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT | Intent.FLAG_ACTIVITY_SINGLE_TOP);
            startActivity(intent);
        } catch (Throwable ignored) {
        }
    }

    private static int dp(int value, float density) {
        return Math.max(1, Math.round(value * density));
    }
}
