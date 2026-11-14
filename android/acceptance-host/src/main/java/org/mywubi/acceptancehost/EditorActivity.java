package org.mywubi.acceptancehost;

/** Independent OS editor: no JNI, model, IME mocks, or connection overrides. */
public final class EditorActivity extends android.app.Activity {
    @Override public void onCreate(android.os.Bundle state) {
        super.onCreate(state);
        android.widget.EditText editor = new android.widget.EditText(this);
        editor.setId(1);
        editor.setInputType(getIntent().getIntExtra("inputType",
            android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE));
        editor.setGravity(android.view.Gravity.TOP);
        editor.setTextSize(22);
        // Observe real document mutations without replacing the OS InputConnection.
        // This debug-only host lets IME acceptance distinguish local queries from
        // transient text that was written and then removed before a screenshot.
        java.io.File journal = new java.io.File(getExternalFilesDir(null), "edits.jsonl");
        try { new java.io.FileOutputStream(journal, false).close(); }
        catch (java.io.IOException error) { throw new IllegalStateException(error); }
        editor.addTextChangedListener(new android.text.TextWatcher() {
            public void beforeTextChanged(CharSequence text, int start, int count, int after) {}
            public void onTextChanged(CharSequence text, int start, int before, int count) {}
            public void afterTextChanged(android.text.Editable text) {
                try (java.io.FileOutputStream output = new java.io.FileOutputStream(journal, true)) {
                    org.json.JSONObject row = new org.json.JSONObject()
                        .put("text", text.toString())
                        .put("composing_start", android.view.inputmethod.BaseInputConnection.getComposingSpanStart(text))
                        .put("composing_end", android.view.inputmethod.BaseInputConnection.getComposingSpanEnd(text));
                    output.write((row.toString() + "\n").getBytes(java.nio.charset.StandardCharsets.UTF_8));
                } catch (java.io.IOException | org.json.JSONException error) {
                    throw new IllegalStateException(error);
                }
            }
        });
        setContentView(editor);
        editor.requestFocus();
    }
}
