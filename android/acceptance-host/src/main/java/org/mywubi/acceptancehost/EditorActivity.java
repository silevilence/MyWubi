package org.mywubi.acceptancehost;

/** Independent OS editor: no JNI, model, IME mocks, or connection overrides. */
public final class EditorActivity extends android.app.Activity {
    @Override public void onCreate(android.os.Bundle state) {
        super.onCreate(state);
        android.widget.EditText editor = new android.widget.EditText(this);
        editor.setId(1);
        editor.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE);
        editor.setGravity(android.view.Gravity.TOP);
        editor.setTextSize(22);
        setContentView(editor);
        editor.requestFocus();
    }
}
