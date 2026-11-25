package org.mywubi.acceptancehost;

import android.accessibilityservice.AccessibilityServiceInfo;
import android.app.Activity;
import android.app.Instrumentation;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.os.SystemClock;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityWindowInfo;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.ArrayDeque;
import java.util.function.Predicate;
import java.util.stream.Collectors;

/** This fixture runs in the independent host so it survives loss of the IME process. */
public final class SkinTrialRestartCheck extends Instrumentation {
    private static final String IME = "org.mywubi/.MyWubiService";
    @Override public void onCreate(Bundle args) { super.onCreate(args); start(); }

    private String shell(String command) throws Exception {
        try (BufferedReader reader = new BufferedReader(new InputStreamReader(
                new ParcelFileDescriptor.AutoCloseInputStream(getUiAutomation().executeShellCommand(command))))) {
            return reader.lines().collect(Collectors.joining("\n")).trim();
        }
    }

    private AccessibilityNodeInfo find(Predicate<AccessibilityNodeInfo> match) {
        ArrayDeque<AccessibilityNodeInfo> nodes = new ArrayDeque<>();
        for (AccessibilityWindowInfo window : getUiAutomation().getWindows()) {
            AccessibilityNodeInfo root = window.getRoot();
            if (root != null) nodes.add(root);
        }
        while (!nodes.isEmpty()) {
            AccessibilityNodeInfo node = nodes.removeFirst();
            if (match.test(node)) return node;
            for (int i = 0; i < node.getChildCount(); ++i) {
                AccessibilityNodeInfo child = node.getChild(i);
                if (child != null) nodes.add(child);
            }
        }
        return null;
    }

    private AccessibilityNodeInfo await(String label, Predicate<AccessibilityNodeInfo> match) {
        long end = SystemClock.elapsedRealtime() + 12000;
        do {
            AccessibilityNodeInfo node = find(match);
            if (node != null) return node;
            SystemClock.sleep(100);
        } while (SystemClock.elapsedRealtime() < end);
        throw new AssertionError("Missing " + label);
    }

    private void click(String label) {
        AccessibilityNodeInfo node = await(label, n -> String.valueOf(n.getText()).startsWith(label));
        while (node != null && !node.isClickable()) node = node.getParent();
        if (node == null || !node.performAction(AccessibilityNodeInfo.ACTION_CLICK)) throw new AssertionError(label);
    }

    private boolean imeKey(AccessibilityNodeInfo node, String key) {
        return "org.mywubi".contentEquals(String.valueOf(node.getPackageName()))
            && key.contentEquals(String.valueOf(node.getContentDescription())) && node.isEnabled();
    }

    private void checkRestart() throws Exception {
        AccessibilityServiceInfo info = getUiAutomation().getServiceInfo();
        info.flags |= AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS;
        getUiAutomation().setServiceInfo(info);
        String previous = shell("settings get secure default_input_method");
        boolean enabled = java.util.Arrays.asList(shell("ime list -s").split("\\s+")).contains(IME);
        String hash = "run-as org.mywubi sha256sum files/engine/config.toml";
        String original = shell(hash);
        if (!original.matches("[0-9a-f]{64}\\s+.+")) throw new AssertionError("Prepare the preview fixture first");
        try {
            shell("ime enable " + IME);
            shell("ime set " + IME);
            shell("am start -W -f 0x10008000 -n org.mywubi/.MainActivity --es skin.id mywubi.keyboards"
                + " --es skin.version 4 --es skin.component jade --es skin.name restart-test");
            click("键盘 · ");
            click("shape14 · 形码");
            click("试打");
            await("trial GH", n -> imeKey(n, "GH"));
            String pid = shell("pidof org.mywubi");
            if (!pid.matches("[0-9]+")) throw new AssertionError("Missing IME process");
            // Killing only the debug process keeps Android's selected IME intact.
            // Force-stopping the package also tells Android to select another IME.
            String signal = shell("run-as org.mywubi /system/bin/kill -9 " + pid);
            long deadline = SystemClock.elapsedRealtime() + 5000;
            while (pid.equals(shell("pidof org.mywubi")) && SystemClock.elapsedRealtime() < deadline) {
                SystemClock.sleep(100);
            }
            if (pid.equals(shell("pidof org.mywubi"))) throw new AssertionError("Still running: " + pid + " " + signal);
            shell("am start -W -n org.mywubi.acceptancehost/.EditorActivity");
            await("host editor", n -> "android.widget.EditText".contentEquals(String.valueOf(n.getClassName())))
                .performAction(AccessibilityNodeInfo.ACTION_CLICK);
            await("daily q", n -> imeKey(n, "q"));
            String next = shell("pidof org.mywubi");
            if (!next.matches("[0-9]+") || pid.equals(next) || find(n -> imeKey(n, "GH")) != null) {
                throw new AssertionError("Daily keyboard failed to recover in a new process: " + pid + " -> " + next);
            }
            shell("am start -W -f 0x10008000 -n org.mywubi/.MainActivity");
            if (!original.equals(shell(hash))) {
                throw new AssertionError("Trial layout or settings persisted");
            }
        } finally {
            if (previous.contains("/")) shell("ime set " + previous);
            if (!enabled) shell("ime disable " + IME);
        }
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        try {
            checkRestart();
            result.putString("stream", "PASS: trial GH, new IME process, daily q, unchanged configuration");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            result.putString("stream", android.util.Log.getStackTraceString(error));
            finish(Activity.RESULT_CANCELED, result);
        }
    }
}
