package com.trihienkun.laucher;

import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.net.ConnectivityManager;
import android.net.LinkProperties;
import android.net.Network;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.Message;
import android.os.Messenger;
import android.view.Surface;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.List;

public final class GameJvmService extends Service {
    static {
        ThkNativeBridge.ensureLoaded();
    }

    public static final int MSG_START_JVM = 1;
    public static final int MSG_UPDATE_SURFACE = 2;
    public static final int MSG_STOP_JVM = 3;

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
        ThkNativeBridge.nativeNotifyJvmExit(code);
    }

    @SuppressWarnings("deprecation")
    private static Surface extractSurface(Bundle bundle) {
        if (bundle == null) {
            return null;
        }
        bundle.setClassLoader(Surface.class.getClassLoader());
        return bundle.getParcelable("surface");
    }

    @SuppressWarnings("deprecation")
    private String[] queryActiveDns() {
        List<String> dnsList = new ArrayList<>();
        try {
            ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
            if (cm != null) {
                Network active = cm.getActiveNetwork();
                if (active != null) {
                    collectDnsServers(cm.getLinkProperties(active), dnsList);
                }
                Network[] all = cm.getAllNetworks();
                if (all != null) {
                    for (Network net : all) {
                        if (net != null && !net.equals(active)) {
                            collectDnsServers(cm.getLinkProperties(net), dnsList);
                        }
                    }
                }
            }
        } catch (Throwable ignored) {
        }
        return dnsList.toArray(new String[0]);
    }

    private static void collectDnsServers(LinkProperties lp, List<String> out) {
        if (lp == null) {
            return;
        }
        for (InetAddress addr : lp.getDnsServers()) {
            if (addr == null || addr.isLoopbackAddress() || addr.isLinkLocalAddress()) {
                continue;
            }
            String raw = addr.getHostAddress();
            if (raw == null) {
                continue;
            }
            int pct = raw.indexOf('%');
            String clean = (pct >= 0 ? raw.substring(0, pct) : raw).trim();
            if (!clean.isEmpty() && !out.contains(clean)) {
                out.add(clean);
            }
        }
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
                    ThkNativeBridge.nativeStartGameJvm(
                        getApplicationContext(),
                        getFilesDir().getAbsolutePath(),
                        getApplicationInfo().sourceDir,
                        specPath != null ? specPath : "",
                        s,
                        sw,
                        sh,
                        queryActiveDns()
                    );
                    break;
                }
                case MSG_UPDATE_SURFACE: {
                    int sw = data != null ? data.getInt("width", 0) : 0;
                    int sh = data != null ? data.getInt("height", 0) : 0;
                    Surface s = extractSurface(data);
                    ThkNativeBridge.nativeUpdateGameSurface(s, sw, sh);
                    break;
                }
                case MSG_STOP_JVM: {
                    stopSelf();
                    ThkNativeBridge.nativeStopGameJvm();
                    break;
                }
                default:
                    super.handleMessage(msg);
                    break;
            }
        }
    }
}
