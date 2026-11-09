package com.trihienkun.laucher;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.LinkProperties;
import android.net.Network;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetAddress;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

public final class MobileRuntimeInstaller {
    public static final String ASSET_BUNDLE_NAME = "mobile_runtime_arm64.zip";
    public static final String EXPECTED_VERSION = "thk-mobile-runtime-1.1.8-arm64-v1";

    private static final Object LOCK = new Object();

    private MobileRuntimeInstaller() {
    }

    public static File runtimeRootDir(Context ctx) {
        return new File(ctx.getFilesDir(), ".TriHienKunLaucher/runtimes/mobile_runtime");
    }

    public static File ensureExtracted(Context ctx) throws IOException {
        synchronized (LOCK) {
            File root = runtimeRootDir(ctx);
            File verFile = new File(root, "version.txt");
            File pojavExec = new File(root, "lib/libpojavexec.so");
            File gl4es = new File(root, "lib/libng_gl4es.so");
            File lwjglJar = new File(root, "lwjgl3/3.3.3/lwjgl.jar");
            File lwjglGlfwJar = new File(root, "lwjgl3/3.3.3/lwjgl-glfw.jar");

            if (verFile.isFile() && pojavExec.isFile() && gl4es.isFile() && lwjglJar.isFile() && lwjglGlfwJar.isFile()) {
                String currentVer = readUtf8Small(verFile).trim();
                if (EXPECTED_VERSION.equals(currentVer)) {
                    writeResolvConf(ctx, root);
                    return root;
                }
            }

            File parent = root.getParentFile();
            if (parent != null && !parent.exists()) {
                //noinspection ResultOfMethodCallIgnored
                parent.mkdirs();
            }

            File stage = new File(ctx.getFilesDir(), ".TriHienKunLaucher/runtimes/.mobile_runtime_stage");
            deleteRecursive(stage);
            if (!stage.mkdirs()) {
                throw new IOException("Cannot create mobile runtime staging directory: " + stage);
            }

            String canonStage = stage.getCanonicalPath() + File.separator;
            byte[] buffer = new byte[65536];

            try (InputStream assetIn = ctx.getAssets().open(ASSET_BUNDLE_NAME);
                 ZipInputStream zis = new ZipInputStream(assetIn)) {
                ZipEntry entry;
                while ((entry = zis.getNextEntry()) != null) {
                    String normalized = entry.getName().replace('\\', '/');
                    if (normalized.isEmpty() || normalized.startsWith("/") || normalized.contains("..")) {
                        zis.closeEntry();
                        continue;
                    }
                    File out = new File(stage, normalized);
                    String canonOut = out.getCanonicalPath();
                    if (!canonOut.startsWith(canonStage)) {
                        throw new IOException("Blocked path traversal entry in runtime bundle: " + normalized);
                    }
                    if (entry.isDirectory()) {
                        //noinspection ResultOfMethodCallIgnored
                        out.mkdirs();
                    } else {
                        File outParent = out.getParentFile();
                        if (outParent != null && !outParent.exists()) {
                            //noinspection ResultOfMethodCallIgnored
                            outParent.mkdirs();
                        }
                        try (FileOutputStream fos = new FileOutputStream(out)) {
                            int n;
                            while ((n = zis.read(buffer)) != -1) {
                                fos.write(buffer, 0, n);
                            }
                            fos.flush();
                        }
                        //noinspection ResultOfMethodCallIgnored
                        out.setReadable(true, false);
                        String lowerName = normalized.toLowerCase(Locale.ROOT);
                        if (lowerName.endsWith(".so")) {
                            //noinspection ResultOfMethodCallIgnored
                            out.setExecutable(true, false);
                        }
                        if (lowerName.endsWith(".so") || lowerName.endsWith(".jar") || lowerName.endsWith(".dex")) {
                            //noinspection ResultOfMethodCallIgnored
                            out.setWritable(false, false);
                        }
                    }
                    zis.closeEntry();
                }
            }

            File stagedVer = new File(stage, "version.txt");
            //noinspection ResultOfMethodCallIgnored
            stagedVer.setWritable(true, true);
            try (FileOutputStream vfos = new FileOutputStream(stagedVer)) {
                vfos.write(EXPECTED_VERSION.getBytes(StandardCharsets.UTF_8));
                vfos.flush();
            }
            //noinspection ResultOfMethodCallIgnored
            stagedVer.setReadable(true, false);

            deleteRecursive(root);
            if (!stage.renameTo(root)) {
                throw new IOException("Failed to finalize mobile runtime directory at " + root);
            }
            writeResolvConf(ctx, root);
            return root;
        }
    }

    public static File writeResolvConf(Context ctx, File runtimeRoot) {
        File resolv = new File(runtimeRoot, "resolv.conf");
        Set<String> dns = new LinkedHashSet<>();
        try {
            ConnectivityManager cm = (ConnectivityManager) ctx.getSystemService(Context.CONNECTIVITY_SERVICE);
            if (cm != null) {
                Network active = cm.getActiveNetwork();
                if (active != null) {
                    LinkProperties lp = cm.getLinkProperties(active);
                    if (lp != null) {
                        for (InetAddress addr : lp.getDnsServers()) {
                            if (addr != null && addr.getHostAddress() != null) {
                                String host = addr.getHostAddress();
                                int pct = host.indexOf('%');
                                String clean = (pct >= 0 ? host.substring(0, pct) : host).trim();
                                if (!clean.isEmpty()) {
                                    dns.add(clean);
                                }
                            }
                        }
                    }
                }
            }
        } catch (Throwable ignored) {
        }
        dns.add("1.1.1.1");
        dns.add("8.8.8.8");

        StringBuilder sb = new StringBuilder();
        for (String server : dns) {
            sb.append("nameserver ").append(server).append('\n');
        }
        try (FileOutputStream fos = new FileOutputStream(resolv)) {
            fos.write(sb.toString().getBytes(StandardCharsets.UTF_8));
            fos.flush();
        } catch (Throwable ignored) {
        }
        return resolv;
    }

    private static String readUtf8Small(File file) {
        try (FileInputStream in = new FileInputStream(file);
             ByteArrayOutputStream bos = new ByteArrayOutputStream()) {
            byte[] buf = new byte[1024];
            int n;
            while ((n = in.read(buf)) != -1) {
                bos.write(buf, 0, n);
            }
            return bos.toString(StandardCharsets.UTF_8.name()).replace("\uFEFF", "");
        } catch (Throwable ignored) {
            return "";
        }
    }

    private static void deleteRecursive(File target) {
        if (target == null || !target.exists()) {
            return;
        }
        //noinspection ResultOfMethodCallIgnored
        target.setWritable(true, true);
        if (target.isDirectory()) {
            File[] children = target.listFiles();
            if (children != null) {
                for (File child : children) {
                    deleteRecursive(child);
                }
            }
        }
        //noinspection ResultOfMethodCallIgnored
        target.setWritable(true, true);
        //noinspection ResultOfMethodCallIgnored
        target.delete();
    }
}
