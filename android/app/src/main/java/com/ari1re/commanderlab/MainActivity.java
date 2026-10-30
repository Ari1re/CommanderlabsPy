package com.ari1re.commanderlab;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.graphics.Color;
import android.os.Bundle;
import android.util.Log;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.InputMethodManager;
import android.widget.Button;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;

import com.termux.terminal.TerminalSession;
import com.termux.terminal.TerminalSessionClient;
import com.termux.view.TerminalView;
import com.termux.view.TerminalViewClient;

import java.io.File;

/**
 * Hosts the full Commander Lab terminal app.
 *
 * The Rust binary is packaged as lib/<abi>/libcommanderlab.so so Android
 * extracts it into the (executable) native library directory. It runs inside
 * a real pseudo-terminal rendered by Termux's terminal-view, with its working
 * directory in app-private storage — decks, card cache and exports stay on the
 * device.
 */
public class MainActivity extends Activity implements TerminalSessionClient, TerminalViewClient {

    private static final String TAG = "CommanderLab";

    private TerminalView terminalView;
    private TerminalSession session;
    private float fontPx;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        fontPx = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, 11,
                getResources().getDisplayMetrics());

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.BLACK);

        terminalView = new TerminalView(this, null);
        terminalView.setTerminalViewClient(this);
        terminalView.setTextSize((int) fontPx);
        terminalView.setFocusable(true);
        terminalView.setFocusableInTouchMode(true);
        root.addView(terminalView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

        root.addView(buildExtraKeys());
        setContentView(root);

        session = startSession();
        terminalView.attachSession(session);
        terminalView.requestFocus();
    }

    private TerminalSession startSession() {
        File home = getFilesDir();
        File binary = new File(getApplicationInfo().nativeLibraryDir, "libcommanderlab.so");

        String[] env = new String[] {
                "TERM=xterm-256color",
                "COLORTERM=truecolor",
                "HOME=" + home.getAbsolutePath(),
                "TMPDIR=" + getCacheDir().getAbsolutePath(),
                "LANG=en_US.UTF-8",
                "COMMANDER_LAB_NO_AI=1",
                "COMMANDER_LAB_PLATFORM=android",
                "PATH=/system/bin",
        };

        return new TerminalSession(binary.getAbsolutePath(), home.getAbsolutePath(),
                new String[] { "commander-lab" }, env, 2000, this);
    }

    // --- Extra key row: keys a phone keyboard lacks ---------------------

    private View buildExtraKeys() {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.CENTER_VERTICAL);

        addKey(row, "ESC", "\u001b");
        addKey(row, "TAB", "\t");
        addKey(row, "←", "\u001b[D");
        addKey(row, "↑", "\u001b[A");
        addKey(row, "↓", "\u001b[B");
        addKey(row, "→", "\u001b[C");
        addKey(row, "↵", "\r");
        addKey(row, "H", "h");
        addKey(row, "L", "l");
        addKey(row, "[", "[");
        addKey(row, "]", "]");
        addKey(row, "PGUP", "\u001b[5~");
        addKey(row, "PGDN", "\u001b[6~");

        Button keyboard = makeButton("⌨");
        keyboard.setOnClickListener(v -> toggleKeyboard());
        row.addView(keyboard);

        Button zoomOut = makeButton("A-");
        zoomOut.setOnClickListener(v -> changeFont(0.9f));
        row.addView(zoomOut);

        Button zoomIn = makeButton("A+");
        zoomIn.setOnClickListener(v -> changeFont(1.1f));
        row.addView(zoomIn);

        HorizontalScrollView scroll = new HorizontalScrollView(this);
        scroll.setBackgroundColor(Color.rgb(24, 20, 34));
        scroll.addView(row);
        return scroll;
    }

    private Button makeButton(String label) {
        Button button = new Button(this);
        button.setText(label);
        button.setAllCaps(false);
        button.setTextColor(Color.rgb(224, 176, 74));
        button.setBackgroundColor(Color.TRANSPARENT);
        button.setMinWidth(0);
        button.setMinimumWidth(0);
        button.setPadding(28, 8, 28, 8);
        return button;
    }

    private void addKey(LinearLayout row, String label, String sequence) {
        Button button = makeButton(label);
        button.setOnClickListener(v -> {
            if (session != null && session.isRunning()) {
                session.write(sequence);
            }
        });
        row.addView(button);
    }

    private void toggleKeyboard() {
        InputMethodManager imm = (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
        if (imm != null) {
            terminalView.requestFocus();
            imm.toggleSoftInput(InputMethodManager.SHOW_IMPLICIT, 0);
        }
    }

    private void changeFont(float factor) {
        fontPx = Math.max(12f, Math.min(64f, fontPx * factor));
        terminalView.setTextSize((int) fontPx);
    }

    // --- TerminalSessionClient ------------------------------------------

    public void onTextChanged(TerminalSession changedSession) {
        terminalView.onScreenUpdated();
    }

    public void onTitleChanged(TerminalSession changedSession) { }

    public void onSessionFinished(TerminalSession finishedSession) {
        finish();
    }

    public void onCopyTextToClipboard(TerminalSession s, String text) {
        ClipboardManager clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
        if (clipboard != null) {
            clipboard.setPrimaryClip(ClipData.newPlainText("Commander Lab", text));
        }
    }

    public void onPasteTextFromClipboard(TerminalSession s) {
        ClipboardManager clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
        if (clipboard == null || !clipboard.hasPrimaryClip() || session == null) return;
        ClipData clip = clipboard.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) return;
        CharSequence text = clip.getItemAt(0).coerceToText(this);
        if (text != null) session.getEmulator().paste(text.toString());
    }

    public void onBell(TerminalSession s) { }

    public void onColorsChanged(TerminalSession s) {
        terminalView.invalidate();
    }

    public void onTerminalCursorStateChange(boolean state) { }

    public void setTerminalShellPid(TerminalSession s, int pid) { }

    public Integer getTerminalCursorStyle() {
        return null;
    }

    // --- TerminalViewClient ---------------------------------------------

    public float onScale(float scale) {
        if (scale < 0.9f || scale > 1.1f) {
            changeFont(scale);
            return 1.0f;
        }
        return scale;
    }

    public void onSingleTapUp(MotionEvent e) {
        // Taps become mouse clicks when the app enables mouse tracking;
        // otherwise bring up the keyboard.
        if (session == null || !session.getEmulator().isMouseTrackingActive()) {
            toggleKeyboard();
        }
    }

    public boolean shouldBackButtonBeMappedToEscape() { return true; }

    public boolean shouldEnforceCharBasedInput() { return true; }

    public boolean shouldUseCtrlSpaceWorkaround() { return false; }

    public boolean isTerminalViewSelected() { return true; }

    public void copyModeChanged(boolean copyMode) { }

    public boolean onKeyDown(int keyCode, KeyEvent e, TerminalSession s) { return false; }

    public boolean onKeyUp(int keyCode, KeyEvent e) { return false; }

    public boolean onLongPress(MotionEvent event) { return false; }

    public boolean readControlKey() { return false; }

    public boolean readAltKey() { return false; }

    public boolean readShiftKey() { return false; }

    public boolean readFnKey() { return false; }

    public boolean onCodePoint(int codePoint, boolean ctrlDown, TerminalSession s) { return false; }

    public void onEmulatorSet() { }

    // --- Logging (shared by both client interfaces) -----------------------

    public void logError(String tag, String message) { Log.e(tag, message); }

    public void logWarn(String tag, String message) { Log.w(tag, message); }

    public void logInfo(String tag, String message) { Log.i(tag, message); }

    public void logDebug(String tag, String message) { Log.d(tag, message); }

    public void logVerbose(String tag, String message) { Log.v(tag, message); }

    public void logStackTraceWithMessage(String tag, String message, Exception e) {
        Log.e(tag, message, e);
    }

    public void logStackTrace(String tag, Exception e) { Log.e(tag, "error", e); }

    @Override
    protected void onDestroy() {
        if (session != null && isFinishing()) {
            session.finishIfRunning();
        }
        super.onDestroy();
    }
}
