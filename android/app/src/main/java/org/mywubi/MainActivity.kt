package org.mywubi

import android.app.Activity
import android.os.Bundle
import android.content.Intent
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(48, 80, 48, 48)
            addView(TextView(context).apply { text = "MyWubi\n形码 · 拼音 · 英文"; textSize = 28f })
            addView(TextView(context).apply { text = "首次使用请启用并选择输入法。\n当前内置五笔表是开发联调示例表。"; textSize = 16f })
            addView(Button(context).apply { text = "1. 启用 MyWubi"; setOnClickListener { startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS)) } })
            addView(Button(context).apply { text = "2. 选择输入法"; setOnClickListener { getSystemService(InputMethodManager::class.java).showInputMethodPicker() } })
        })
    }
}
