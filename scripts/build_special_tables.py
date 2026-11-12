"""Build curated special resources offline. Output and ordering are versioned data."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = "1.0.0"
NOTICE = (ROOT / "expressions/licenses/MyWubi-MIT.txt").read_text(encoding="utf-8")

# Original MyWubi curation: individual kana, not a romaji transliteration engine.
KANA = """
あ:a い:i う:u え:e お:o
か:ka き:ki く:ku け:ke こ:ko
さ:sa し:shi,si す:su せ:se そ:so
た:ta ち:chi,ti つ:tsu,tu て:te と:to
な:na に:ni ぬ:nu ね:ne の:no
は:ha ひ:hi ふ:fu,hu へ:he ほ:ho
ま:ma み:mi む:mu め:me も:mo
や:ya ゆ:yu よ:yo
ら:ra り:ri る:ru れ:re ろ:ro
わ:wa を:wo ん:n,nn
が:ga ぎ:gi ぐ:gu げ:ge ご:go
ざ:za じ:ji,zi ず:zu ぜ:ze ぞ:zo
だ:da ぢ:di,ji づ:du,zu で:de ど:do
ば:ba び:bi ぶ:bu べ:be ぼ:bo
ぱ:pa ぴ:pi ぷ:pu ぺ:pe ぽ:po ゔ:vu
ぁ:xa,la ぃ:xi,li ぅ:xu,lu ぇ:xe,le ぉ:xo,lo
っ:xtsu,xtu,ltsu ゃ:xya,lya ゅ:xyu,lyu ょ:xyo,lyo ゎ:xwa,lwa ゕ:xka ゖ:xke
ゐ:wi ゑ:we
""".split()


def kana_entries():
    # Explicit pair table keeps combining variants independent from their composed form.
    voiced = dict(zip("がぎぐげござじずぜぞだぢづでどばびぶべぼゔ", "かきくけこさしすせそたちつてとはひふへほう"))
    semivoiced = dict(zip("ぱぴぷぺぽ", "はひふへほ"))
    rows = []
    for offset, category in [(0, "平假名"), (0x60, "片假名")]:
        for pair in KANA:
            hira, readings = pair.split(":")
            text = chr(ord(hira) + offset)
            tags = ["假名", category]
            if hira in "ぁぃぅぇぉっゃゅょゎゕゖ":
                tags.append("小写假名")
            if hira in "ゐゑ":
                tags.append("历史假名")
            if hira in voiced:
                tags.append("浊音")
            if hira in semivoiced:
                tags.append("半浊音")
            rows.append((text, category + " " + readings.split(",")[0], tags, readings.split(",")))
        for pair in KANA:
            hira, readings = pair.split(":")
            base = voiced.get(hira) or semivoiced.get(hira)
            if base:
                mark = "\u3099" if hira in voiced else "\u309a"
                text = chr(ord(base) + offset) + mark
                rows.append((text, category + " " + readings.split(",")[0] + " 组合形式",
                             ["假名", category, "组合形式", "浊音" if hira in voiced else "半浊音"], readings.split(",")))
    return rows


def curated_rows(data, category):
    rows = []
    for line in data.strip().splitlines():
        text, name, aliases = line.strip().split("|")
        rows.append((text, name, [category], aliases.split(",")))
    return rows


def math_entries():
    rows = curated_rows("""
+|加号|plus,加
−|减号|minus,减
±|正负号|plusminus,正负
∓|负正号|minusplus,负正
×|乘号|times,乘
÷|除号|divide,除
·|点乘|dot,点乘
＝|全角等号|fullwidth equal
=|等号|equal,等于
≠|不等号|not equal,不等于
≈|约等号|approximately,约等于
≡|恒等号|identical,恒等
≤|小于等于|less equal
≥|大于等于|greater equal
≪|远小于|much less
≫|远大于|much greater
<|小于号|less
>|大于号|greater
∞|无穷大|infinity,无穷
∑|求和|sum,summation
∏|连乘|product
∫|积分|integral
∬|二重积分|double integral
∭|三重积分|triple integral
∮|围道积分|contour integral
√|平方根|sqrt,根号
∛|立方根|cube root
∜|四次方根|fourth root
∂|偏导数|partial
∇|梯度|nabla,gradient
∆|增量|increment
∈|属于|element,属于集合
∉|不属于|not element
∋|包含元素|contains element
∅|空集|empty set
∩|交集|intersection
∪|并集|union
⊂|真子集|subset
⊃|真超集|superset
⊆|子集或相等|subset equal
⊇|超集或相等|superset equal
∀|任意|forall,全称量词
∃|存在|exists,存在量词
∄|不存在|not exists
¬|逻辑非|not
∧|逻辑与|and
∨|逻辑或|or
⊕|直和|direct sum
⊗|张量积|tensor product
⇒|推出|implies
⇔|当且仅当|iff
∴|所以|therefore
∵|因为|because
∝|正比于|proportional
⊥|垂直|perpendicular
∥|平行|parallel
∠|角|angle
°|度数|degree
′|角分|prime
″|角秒|double prime
‰|千分号|permille
‱|万分号|basis point
¹|上标一|superscript1
²|平方上标|superscript2
³|立方上标|superscript3
⁰|上标零|superscript0
₀|下标零|subscript0
₁|下标一|subscript1
₂|下标二|subscript2
₃|下标三|subscript3
ℕ|自然数集|naturals
ℤ|整数集|integers
ℚ|有理数集|rationals
ℝ|实数集|reals
ℂ|复数集|complex
""", "数学符号")
    letters = zip("ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ", "αβγδεζηθικλμνξοπρστυφχψω",
                  "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega".split())
    for upper, lower, reading in letters:
        rows.extend([(upper, "大写希腊字母 " + reading, ["数学符号", "希腊字母", "大写希腊字母"], [reading]),
                     (lower, "小写希腊字母 " + reading, ["数学符号", "希腊字母", "小写希腊字母"], [reading])])
    rows.append(("ς", "词尾西格马", ["数学符号", "希腊字母"], ["final sigma"]))
    return rows


def punctuation_entries():
    return curated_rows("""
，|中文逗号|逗号,comma
。|中文句号|句号,period
、|顿号|enumeration comma
；|中文分号|分号,semicolon
：|中文冒号|冒号,colon
？|中文问号|问号,question
！|中文感叹号|感叹号,exclamation
……|中文省略号|省略号,ellipsis
——|中文破折号|破折号,dash
…|单省略号|single ellipsis
—|长破折号|em dash
–|短破折号|en dash
「|左直角引号|左引号,left corner quote
」|右直角引号|右引号,right corner quote
「」|成对直角引号|引号,corner quotes
『|左双直角引号|left double corner quote
』|右双直角引号|right double corner quote
『』|成对双直角引号|双引号,double corner quotes
“|左双引号|left double quote
”|右双引号|right double quote
“”|成对双引号|双引号,double quotes
‘|左单引号|left single quote
’|右单引号|right single quote
‘’|成对单引号|单引号,single quotes
（|左圆括号|left parenthesis
）|右圆括号|right parenthesis
（）|成对圆括号|圆括号,parentheses
【|左方头括号|left bracket
】|右方头括号|right bracket
【】|成对方头括号|方括号,brackets
《|左书名号|left title mark
》|右书名号|right title mark
《》|成对书名号|书名号,title marks
〈|左单书名号|left angle bracket
〉|右单书名号|right angle bracket
〈〉|成对单书名号|单书名号,angle brackets
〔|左六角括号|left tortoise bracket
〕|右六角括号|right tortoise bracket
〔〕|成对六角括号|六角括号,tortoise brackets
,|半角逗号|ascii comma
.|半角句点|ascii period
;|半角分号|ascii semicolon
:|半角冒号|ascii colon
?|半角问号|ascii question
!|半角感叹号|ascii exclamation
...|半角省略号|ascii ellipsis
()|半角圆括号|ascii parentheses
[]|半角方括号|ascii brackets
{}|半角花括号|ascii braces
""", "标点")


def symbol_entries():
    rows = curated_rows("""
←|左箭头|left arrow,箭头
→|右箭头|right arrow,箭头
↑|上箭头|up arrow,箭头
↓|下箭头|down arrow,箭头
↔|左右箭头|left right arrow
↕|上下箭头|up down arrow
↖|左上箭头|north west arrow
↗|右上箭头|north east arrow
↘|右下箭头|south east arrow
↙|左下箭头|south west arrow
⇐|左双箭头|left double arrow
⇒|右双箭头|right double arrow
⇑|上双箭头|up double arrow
⇓|下双箭头|down double arrow
⇔|左右双箭头|left right double arrow
↩|左回转箭头|return arrow
↪|右回转箭头|right return arrow
℃|摄氏度|celsius,温度
℉|华氏度|fahrenheit,温度
°C|摄氏温标字母形式|celsius letters
°F|华氏温标字母形式|fahrenheit letters
㎜|毫米单位符号|millimetre
㎝|厘米单位符号|centimetre
㎞|千米单位符号|kilometre
㎡|平方米单位符号|square metre
㎥|立方米单位符号|cubic metre
㎎|毫克单位符号|milligram
㎏|千克单位符号|kilogram
㏄|立方厘米单位符号|cubic centimetre
㏑|自然对数符号|ln
㏒|对数符号|log
￥|全角人民币符号|人民币,yuan
¥|日元符号|yen
€|欧元符号|euro
£|英镑符号|pound
¢|分币符号|cent
$|美元符号|dollar
©|版权符号|copyright
®|注册商标|registered
™|商标符号|trademark
№|编号符号|number
§|章节符号|section
¶|段落符号|paragraph
※|参考标记|reference
★|实心五角星|星号,star
☆|空心五角星|outline star
●|实心圆|circle
○|空心圆|outline circle
■|实心方块|square
□|空心方块|outline square
▲|实心上三角|triangle
△|空心上三角|outline triangle
◆|实心菱形|diamond
◇|空心菱形|outline diamond
✓|勾号|check
✗|叉号|cross
♂|雄性符号|male
♀|雌性符号|female
♠|黑桃|spade
♥|红桃|heart
♣|梅花|club
♦|方块花色|diamond suit
♩|四分音符|quarter note
♪|八分音符|eighth note
♫|双音符|music
""", "特殊符号")
    for i in range(20):
        rows.append((chr(0x2460+i), f"带圈数字{i+1}", ["特殊符号", "序号", "带圈数字"], [f"circled{i+1}"]))
    for i in range(12):
        rows.append((chr(0x2160+i), f"大写罗马数字{i+1}", ["特殊符号", "序号", "罗马数字"], [f"roman{i+1}"]))
        rows.append((chr(0x2170+i), f"小写罗马数字{i+1}", ["特殊符号", "序号", "罗马数字"], [f"roman{i+1}"]))
    return rows


def write_resource(kind, name, rows):
    entries = []
    for order, (text, label, tags, aliases) in enumerate(rows):
        chinese_aliases = [alias for alias in aliases if any("\u4e00" <= c <= "\u9fff" for c in alias)]
        entries.append(dict(text=text, name=label, triggers=list(dict.fromkeys([label, *tags, *chinese_aliases])), tags=tags,
                            aliases=aliases, order=order, sources=["mywubi-curated"]))
    assert len({row["text"] for row in entries}) == len(entries)
    document = dict(kind=kind, name=name, version=VERSION,
        sources=[dict(id="mywubi-curated", url="local:scripts/build_special_tables.py",
                      revision=VERSION, license="MIT", notice=NOTICE)], entries=entries)
    encoded = "# MyWubi Expressions 1\n" + json.dumps(document, ensure_ascii=False, indent=2) + "\n"
    target = ROOT / "expressions" / (kind + ".mwe")
    target.write_bytes(encoded.encode("utf-8"))
    coverage = dict(kind=kind, version=VERSION, count=len(entries),
                    sha256=hashlib.sha256(target.read_bytes()).hexdigest(),
                    outputs=[row["text"] for row in entries],
                    queries={q: [row["text"] for row in entries if q in row["aliases"]]
                             for q in sorted({q for row in entries for q in row["aliases"]})})
    (ROOT / "expressions" / (kind + "-coverage.json")).write_bytes(
        (json.dumps(coverage, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    print(kind, len(entries), coverage["sha256"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["kana", "math", "punctuation", "symbols"])
    options = parser.parse_args()
    if options.kind == "kana":
        write_resource("kana", "MyWubi 假名字符", kana_entries())
    else:
        generators = {"math": ("MyWubi 数学符号", math_entries),
                      "punctuation": ("MyWubi 标点", punctuation_entries),
                      "symbols": ("MyWubi 特殊符号", symbol_entries)}
        title, generator = generators[options.kind]
        write_resource(options.kind, title, generator())
