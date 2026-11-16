package org.mywubi

/** Shared inventory for settings, resources, capability decoding and IME pickers. */
internal data class SpecialCategory(val mode:String, val kind:String, val label:String)
internal val specialCategoryDefinitions = listOf(
    SpecialCategory("Emoji", "emoji", "Emoji"),
    SpecialCategory("Kaomoji", "kaomoji", "颜文字"),
    SpecialCategory("Kana", "kana", "假名"),
    SpecialCategory("Math", "math", "数学符号"),
    SpecialCategory("Punctuation", "punctuation", "标点"),
    SpecialCategory("Symbols", "symbols", "其他符号"),
    SpecialCategory("Greek", "greek", "希腊字母"),
    SpecialCategory("Cyrillic", "cyrillic", "西里尔字母"),
    SpecialCategory("Latin", "latin", "拼音与拉丁"),
    SpecialCategory("Phonetic", "phonetic", "音标"),
    SpecialCategory("Enclosed", "enclosed", "圈号与括号"),
    SpecialCategory("Numbers", "numbers", "数字形式"),
    SpecialCategory("Arrows", "arrows", "箭头"),
    SpecialCategory("Currency", "currency", "货币"),
    SpecialCategory("Units", "units", "单位"),
    SpecialCategory("Shapes", "shapes", "几何图形"),
    SpecialCategory("BoxDrawing", "box_drawing", "制表与方块"),
    SpecialCategory("Braille", "braille", "盲文"),
    SpecialCategory("Music", "music", "音乐符号"),
    SpecialCategory("Diacritics", "diacritics", "组合附加符")
)
val specialCategories = specialCategoryDefinitions.map { it.kind to it.label }
internal val specialModes = specialCategoryDefinitions.map { it.mode to it.label }
internal val specialKinds = specialCategoryDefinitions.map { it.kind }
internal fun specialLabel(kind:String) = specialCategoryDefinitions.firstOrNull { it.kind==kind }?.label ?: "特殊输入"
internal fun specialKind(mode:String) = specialCategoryDefinitions.firstOrNull { it.mode==mode }?.kind ?: mode.lowercase(java.util.Locale.ROOT)
