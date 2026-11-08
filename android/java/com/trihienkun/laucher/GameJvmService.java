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
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Iterator;
import java.util.concurrent.atomic.AtomicBoolean;

public final class GameJvmService extends Service {
    public static final int MSG_START_JVM = 1;
    public static final int MSG_UPDATE_SURFACE = 2;
    public static final int MSG_STOP_JVM = 3;

    private static final Object IO_LOCK = new Object();
    private static final AtomicBoolean EXITING = new AtomicBoolean(false);
    private static volatile Socket controlSocket;
    private static volatile BufferedWriter controlWriter;

    private final Object surfaceLock = new Object();
    private final AtomicBoolean jvmStarted = new AtomicBoolean(false);
    private volatile Surface currentSurface;
    private volatile boolean bridgeWindowBound = false;
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
                    Surface s = extractSurface(data);
                    updateSurfaceInternal(s);
                    if (!specPath.isEmpty() && jvmStarted.compareAndSet(false, true)) {
                        Thread worker = new Thread(() -> runJvmMain(specPath), "thk-jvm-main");
                        worker.start();
                    }
                    break;
                }
                case MSG_UPDATE_SURFACE: {
                    Surface s = extractSurface(data);
                    updateSurfaceInternal(s);
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

    private void updateSurfaceInternal(Surface incoming) {
        synchronized (surfaceLock) {
            currentSurface = incoming;
            surfaceLock.notifyAll();
            if (bridgeWindowBound && incoming != null && incoming.isValid() && JREUtils.isBridgeLoaded()) {
                try {
                    JREUtils.setupBridgeWindow(incoming);
                } catch (Throwable ignored) {
                }
            }
        }
    }

    private Surface awaitValidSurface(long timeoutMs) {
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
                    break;
                }
            }
            return currentSurface;
        }
    }

    private void runJvmMain(String specPath) {
        Context appCtx = getApplicationContext();
        try {
            MobileRuntimeInstaller.ensureExtracted(appCtx);
            String rawSpec = readUtf8(new File(specPath));
            if (rawSpec.isEmpty()) {
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
            JREUtils.ensureBridgeLibrariesLoaded(bridgeLibDir);
            if (!configuredLdPath.isEmpty()) {
                JREUtils.setLdLibraryPath(configuredLdPath);
            }

            CallbackBridge.attachContext(appCtx);
            CallbackBridge.windowWidth = windowW;
            CallbackBridge.windowHeight = windowH;
            CallbackBridge.physicalWidth = physW;
            CallbackBridge.physicalHeight = physH;
            CallbackBridge.addGrabListener(grabbing -> sendControlLine("GRAB " + (grabbing ? "1" : "0")));

            Thread rxThread = new Thread(() -> runInputReceiver(sock), "thk-input-rx");
            rxThread.setDaemon(true);
            rxThread.start();

            Logger.begin(logFile);

            Surface surface = awaitValidSurface(6000L);
            synchronized (surfaceLock) {
                if (surface != null && surface.isValid()) {
                    JREUtils.setupBridgeWindow(surface);
                    bridgeWindowBound = true;
                }
            }

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

            JREUtils.loadOpenJdkLibraries(
                new File(jreHome),
                bridgeLibDir,
                new File(lwjglNativesDir)
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

            int rc = VMLauncher.launchJVM(argv);
            notifyJvmExit(rc);
        } catch (Throwable err) {
            try {
                Logger.appendToLog("[THK] Android JVM bootstrap error: " + err);
            } catch (Throwable ignored) {
            }
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
