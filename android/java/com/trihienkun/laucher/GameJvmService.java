package com.trihienkun.laucher;

import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.Message;
import android.os.Messenger;
import android.os.Process;
import android.system.Os;
import android.util.Log;
import android.view.Surface;

import com.oracle.dalvik.VMLauncher;

import net.kdt.pojavlaunch.Logger;
import net.kdt.pojavlaunch.utils.JREUtils;

import org.json.JSONArray;
import org.json.JSONObject;
import org.lwjgl.glfw.CallbackBridge;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Iterator;
import java.util.concurrent.atomic.AtomicBoolean;

public final class GameJvmService extends Service {
    static {
        try {
            System.loadLibrary("thk_jvm_shim");
        } catch (Throwable ignored) {
        }
    }

    private static final String LOG_TAG = "THKGameJvm";
    private static final Object BOOT_LOG_LOCK = new Object();

    public static final int MSG_START_JVM = 1;
    public static final int MSG_UPDATE_SURFACE = 2;
    public static final int MSG_STOP_JVM = 3;

    private static final Object IO_LOCK = new Object();
    private static final AtomicBoolean EXITING = new AtomicBoolean(false);
    private static volatile Socket controlSocket;
    private static volatile BufferedWriter controlWriter;
    private static volatile boolean loggerActive = false;

    private final Object surfaceLock = new Object();
    private final AtomicBoolean jvmStarted = new AtomicBoolean(false);
    private volatile Surface currentSurface;
    private volatile int currentSurfaceWidth = 0;
    private volatile int currentSurfaceHeight = 0;
    private volatile boolean bridgeWindowReady = false;
    private Messenger messenger;

    @Override
    public void onCreate() {
        super.onCreate();
        messenger = new Messenger(new IncomingHandler(Looper.getMainLooper()));
    }

    @Override
    public IBinder onBind(Intent intent) {
        return messenger.getBinder();
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        return START_NOT_STICKY;
    }

    public static void notifyJvmExit(int code) {
        if (!EXITING.compareAndSet(false, true)) {
            return;
        }
        sendControlLine("EXIT " + code);
        synchronized (IO_LOCK) {
            try {
                if (controlWriter != null) {
                    controlWriter.flush();
                    controlWriter.close();
                }
            } catch (Throwable ignored) {
            }
            try {
                if (controlSocket != null) {
                    controlSocket.close();
                }
            } catch (Throwable ignored) {
            }
            controlWriter = null;
            controlSocket = null;
        }
        try {
            if (JREUtils.isBridgeLoaded()) {
                JREUtils.releaseBridgeWindow();
            }
        } catch (Throwable ignored) {
        }
        Process.killProcess(Process.myPid());
    }

    private static void sendControlLine(String line) {
        synchronized (IO_LOCK) {
            if (controlWriter == null) {
                return;
            }
            try {
                controlWriter.write(line);
                controlWriter.write('\n');
                controlWriter.flush();
            } catch (Throwable ignored) {
            }
        }
    }

    @SuppressWarnings("deprecation")
    private static Surface extractSurface(Bundle bundle) {
        if (bundle == null) {
            return null;
        }
        bundle.setClassLoader(Surface.class.getClassLoader());
        return bundle.getParcelable("surface");
    }

    private final class IncomingHandler extends Handler {
        IncomingHandler(Looper looper) {
            super(looper);
        }

        @Override
        public void handleMessage(Message msg) {
            Bundle data = msg.getData();
            switch (msg.what) {
                case MSG_START_JVM: {
                    String specPath = data != null ? data.getString("spec_path", "") : "";
                    int sw = data != null ? data.getInt("width", 0) : 0;
                    int sh = data != null ? data.getInt("height", 0) : 0;
                    Surface s = extractSurface(data);
                    updateSurfaceInternal(s, sw, sh);
                    if (!specPath.isEmpty() && jvmStarted.compareAndSet(false, true)) {
                        Thread worker = new Thread(() -> runJvmMain(specPath), "thk-jvm-main");
                        worker.start();
                    }
                    break;
                }
                case MSG_UPDATE_SURFACE: {
                    int sw = data != null ? data.getInt("width", 0) : 0;
                    int sh = data != null ? data.getInt("height", 0) : 0;
                    Surface s = extractSurface(data);
                    updateSurfaceInternal(s, sw, sh);
                    break;
                }
                case MSG_STOP_JVM: {
                    stopSelf();
                    notifyJvmExit(0);
                    break;
                }
                default:
                    super.handleMessage(msg);
                    break;
            }
        }
    }

    private static void appendBootstrapLog(File logFile, String msg, Throwable err) {
        if (err != null) {
            Log.e(LOG_TAG, msg, err);
        } else {
            Log.i(LOG_TAG, msg);
        }
        if (logFile == null && !loggerActive) {
            return;
        }
        synchronized (BOOT_LOG_LOCK) {
            try {
                StringBuilder sb = new StringBuilder();
                sb.append("[THK-BOOT] ").append(msg).append('\n');
                if (err != null) {
                    StringWriter sw = new StringWriter();
                    try (PrintWriter pw = new PrintWriter(sw)) {
                        err.printStackTrace(pw);
                    }
                    sb.append(sw);
                    if (sb.charAt(sb.length() - 1) != '\n') {
                        sb.append('\n');
                    }
                }
                if (loggerActive) {
                    Logger.appendToLog(sb.toString().trim());
                } else if (logFile != null) {
                    File parent = logFile.getParentFile();
                    if (parent != null && !parent.exists()) {
                        //noinspection ResultOfMethodCallIgnored
                        parent.mkdirs();
                    }
                    try (FileOutputStream fos = new FileOutputStream(logFile, true)) {
                        fos.write(sb.toString().getBytes(StandardCharsets.UTF_8));
                        fos.flush();
                    }
                }
            } catch (Throwable ignored) {
            }
        }
    }

    private void updateSurfaceInternal(Surface incoming, int width, int height) {
        synchronized (surfaceLock) {
            currentSurface = incoming;
            if (width > 0 && height > 0) {
                currentSurfaceWidth = width;
                currentSurfaceHeight = height;
            }
            surfaceLock.notifyAll();
            if (bridgeWindowReady && incoming != null && incoming.isValid() && JREUtils.isBridgeLoaded()) {
                try {
                    JREUtils.setupBridgeWindow(incoming);
                    if (width > 0
                        && height > 0
                        && (CallbackBridge.physicalWidth != width || CallbackBridge.physicalHeight != height)) {
                        double scale = CallbackBridge.physicalWidth > 0
                            ? ((double) CallbackBridge.windowWidth / (double) CallbackBridge.physicalWidth)
                            : 1.0;
                        if (scale <= 0.1 || scale > 2.0) {
                            scale = 1.0;
                        }
                        int newWinW = Math.max(320, ((int) Math.round(width * scale)) & ~1);
                        int newWinH = Math.max(240, ((int) Math.round(height * scale)) & ~1);
                        CallbackBridge.physicalWidth = width;
                        CallbackBridge.physicalHeight = height;
                        CallbackBridge.windowWidth = newWinW;
                        CallbackBridge.windowHeight = newWinH;
                        CallbackBridge.sendUpdateWindowSize(newWinW, newWinH);
                    }
                    appendBootstrapLog(
                        null,
                        "late surface bound (valid=true, phys=" + width + "x" + height + ")",
                        null
                    );
                } catch (Throwable ignored) {
                }
            }
        }
    }

    private Surface awaitValidSurface(long timeoutMs, int targetWinW, int targetWinH) {
        long deadline = System.currentTimeMillis() + timeoutMs;
        synchronized (surfaceLock) {
            while (currentSurface == null || !currentSurface.isValid()) {
                long remain = deadline - System.currentTimeMillis();
                if (remain <= 0L) {
                    break;
                }
                try {
                    surfaceLock.wait(Math.min(remain, 100L));
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    return currentSurface;
                }
            }
            if (currentSurface != null && currentSurface.isValid() && targetWinW != targetWinH) {
                boolean wantLandscape = targetWinW > targetWinH;
                long orientDeadline = Math.min(deadline, System.currentTimeMillis() + 2500L);
                while (currentSurfaceWidth > 0
                    && currentSurfaceHeight > 0
                    && (currentSurfaceWidth >= currentSurfaceHeight) != wantLandscape) {
                    long remain = orientDeadline - System.currentTimeMillis();
                    if (remain <= 0L) {
                        break;
                    }
                    try {
                        surfaceLock.wait(Math.min(remain, 80L));
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        break;
                    }
                }
            }
            return currentSurface;
        }
    }

    private void runJvmMain(String specPath) {
        Context appCtx = getApplicationContext();
        File logFileObj = new File(new File(appCtx.getFilesDir(), ".thk_bridge"), "mobile_game.log");
        appendBootstrapLog(logFileObj, "bootstrap start (pid=" + Process.myPid() + ")", null);
        try {
            MobileRuntimeInstaller.ensureExtracted(appCtx);
            String rawSpec = readUtf8(new File(specPath));
            if (rawSpec.isEmpty()) {
                appendBootstrapLog(logFileObj, "empty mobile launch spec at " + specPath, null);
                notifyJvmExit(2);
                return;
            }
            JSONObject spec = new JSONObject(rawSpec);
            int inputPort = spec.getInt("input_port");
            String jreHome = spec.getString("jre_home");
            String runtimeRoot = spec.getString("runtime_root");
            String lwjglNativesDir = spec.getString("lwjgl_natives_dir");
            String instanceDir = spec.getString("instance_dir");
            String logFile = spec.getString("log_file");
            if (!logFile.trim().isEmpty()) {
                logFileObj = new File(logFile);
            }
            int windowW = Math.max(320, spec.optInt("window_width", 1280));
            int windowH = Math.max(240, spec.optInt("window_height", 720));
            int physW = Math.max(320, spec.optInt("physical_width", windowW));
            int physH = Math.max(240, spec.optInt("physical_height", windowH));
            String rendererLib = spec.optString("renderer_lib", "").trim();
            String eglLib = spec.isNull("egl_lib") ? "" : spec.optString("egl_lib", "").trim();

            Socket sock = new Socket("127.0.0.1", inputPort);
            sock.setTcpNoDelay(true);
            synchronized (IO_LOCK) {
                controlSocket = sock;
                controlWriter = new BufferedWriter(
                    new OutputStreamWriter(sock.getOutputStream(), StandardCharsets.UTF_8)
                );
            }
            sendControlLine("PID " + Process.myPid());

            JSONObject envObj = spec.optJSONObject("env");
            String configuredLdPath = "";
            if (envObj != null) {
                Iterator<String> keys = envObj.keys();
                while (keys.hasNext()) {
                    String k = keys.next();
                    String v = envObj.optString(k, "");
                    if ("LD_LIBRARY_PATH".equals(k)) {
                        configuredLdPath = v;
                    }
                    try {
                        Os.setenv(k, v, true);
                    } catch (Throwable ignored) {
                    }
                }
            }

            File bridgeLibDir = new File(runtimeRoot, "lib");
            appendBootstrapLog(logFileObj, "loading bridge libraries from " + bridgeLibDir.getAbsolutePath(), null);
            JREUtils.ensureBridgeLibrariesLoaded(bridgeLibDir);
            if (!configuredLdPath.isEmpty()) {
                JREUtils.safeSetLdLibraryPath(configuredLdPath);
            }

            Logger.begin(logFileObj.getAbsolutePath());
            loggerActive = true;
            appendBootstrapLog(logFileObj, "bootstrap start (pid=" + Process.myPid() + ")", null);
            appendBootstrapLog(logFileObj, "bridge loaded", null);

            CallbackBridge.attachContext(appCtx);
            CallbackBridge.windowWidth = windowW;
            CallbackBridge.windowHeight = windowH;
            CallbackBridge.physicalWidth = physW;
            CallbackBridge.physicalHeight = physH;
            CallbackBridge.addGrabListener(grabbing -> sendControlLine("GRAB " + (grabbing ? "1" : "0")));

            Thread rxThread = new Thread(() -> runInputReceiver(sock), "thk-input-rx");
            rxThread.setDaemon(true);
            rxThread.start();

            JREUtils.loadOpenJdkLibraries(
                new File(jreHome),
                bridgeLibDir,
                new File(lwjglNativesDir)
            );
            appendBootstrapLog(logFileObj, "openjdk loaded", null);

            if (!eglLib.isEmpty()) {
                File eglSo = new File(bridgeLibDir, eglLib);
                if (eglSo.isFile()) {
                    JREUtils.dlopen(eglSo.getAbsolutePath());
                }
            }
            if (!rendererLib.isEmpty()) {
                File renSo = new File(bridgeLibDir, rendererLib);
                if (renSo.isFile()) {
                    JREUtils.dlopen(renSo.getAbsolutePath());
                }
            }

            Surface surface = awaitValidSurface(15000L, windowW, windowH);
            boolean surfaceValidNow = false;
            synchronized (surfaceLock) {
                if (currentSurfaceWidth > 0 && currentSurfaceHeight > 0) {
                    double scale = physW > 0 ? ((double) windowW / (double) physW) : 1.0;
                    if (scale <= 0.1 || scale > 2.0) {
                        scale = 1.0;
                    }
                    physW = currentSurfaceWidth;
                    physH = currentSurfaceHeight;
                    windowW = Math.max(320, ((int) Math.round(physW * scale)) & ~1);
                    windowH = Math.max(240, ((int) Math.round(physH * scale)) & ~1);
                    CallbackBridge.physicalWidth = physW;
                    CallbackBridge.physicalHeight = physH;
                    CallbackBridge.windowWidth = windowW;
                    CallbackBridge.windowHeight = windowH;
                }
                bridgeWindowReady = true;
                if (surface != null && surface.isValid()) {
                    JREUtils.setupBridgeWindow(surface);
                    surfaceValidNow = true;
                }
            }
            appendBootstrapLog(
                logFileObj,
                "surface bound (valid=" + surfaceValidNow + ", window=" + windowW + "x" + windowH
                    + ", phys=" + physW + "x" + physH + ")",
                null
            );

            CallbackBridge.nativeSetUseInputStackQueue(true);
            JREUtils.setupExitMethod(appCtx);
            JREUtils.initializeHooks();
            JREUtils.chdir(instanceDir);

            JSONArray argvJson = spec.getJSONArray("argv");
            String[] argv = new String[argvJson.length()];
            for (int i = 0; i < argvJson.length(); i++) {
                argv[i] = argvJson.getString(i);
            }

            appendBootstrapLog(logFileObj, "VMLauncher.launchJVM (argc=" + argv.length + ")", null);
            int rc = VMLauncher.launchJVM(argv);
            appendBootstrapLog(logFileObj, "VMLauncher.launchJVM exited with code " + rc, null);
            notifyJvmExit(rc);
        } catch (Throwable err) {
            appendBootstrapLog(logFileObj, "Android JVM bootstrap error", err);
            notifyJvmExit(1);
        }
    }

    private void runInputReceiver(Socket sock) {
        try (BufferedReader reader = new BufferedReader(
            new InputStreamReader(sock.getInputStream(), StandardCharsets.UTF_8)
        )) {
            String line;
            while ((line = reader.readLine()) != null) {
                String trimmed = line.trim();
                if (trimmed.isEmpty()) {
                    continue;
                }
                if ("Q".equals(trimmed)) {
                    notifyJvmExit(0);
                    return;
                }
                String[] parts = trimmed.split("\\s+");
                if (parts.length == 0) {
                    continue;
                }
                switch (parts[0]) {
                    case "K":
                        if (parts.length >= 3) {
                            int keycode = Integer.parseInt(parts[1]);
                            boolean down = "1".equals(parts[2]);
                            CallbackBridge.sendKeyPress(keycode, down);
                        }
                        break;
                    case "C":
                        if (parts.length >= 3) {
                            int cp = Integer.parseInt(parts[1]);
                            int mods = Integer.parseInt(parts[2]);
                            for (char ch : Character.toChars(cp)) {
                                CallbackBridge.sendChar(ch, mods);
                            }
                        }
                        break;
                    case "M":
                        if (parts.length >= 3) {
                            int btn = Integer.parseInt(parts[1]);
                            boolean down = "1".equals(parts[2]);
                            CallbackBridge.sendMouseButton(btn, down);
                        }
                        break;
                    case "P":
                        if (parts.length >= 3) {
                            float x = Float.parseFloat(parts[1]);
                            float y = Float.parseFloat(parts[2]);
                            CallbackBridge.sendCursorPos(x, y);
                        }
                        break;
                    case "S":
                        if (parts.length >= 3) {
                            double dx = Double.parseDouble(parts[1]);
                            double dy = Double.parseDouble(parts[2]);
                            CallbackBridge.sendScroll(dx, dy);
                        }
                        break;
                    case "W":
                        if (parts.length >= 3) {
                            int w = Integer.parseInt(parts[1]);
                            int h = Integer.parseInt(parts[2]);
                            CallbackBridge.sendUpdateWindowSize(w, h);
                        }
                        break;
                    default:
                        break;
                }
            }
        } catch (Throwable ignored) {
        }
    }

    private static String readUtf8(File file) {
        try (FileInputStream in = new FileInputStream(file);
             ByteArrayOutputStream bos = new ByteArrayOutputStream()) {
            byte[] buf = new byte[4096];
            int n;
            while ((n = in.read(buf)) != -1) {
                bos.write(buf, 0, n);
            }
            return bos.toString(StandardCharsets.UTF_8.name());
        } catch (Throwable ignored) {
            return "";
        }
    }
}
