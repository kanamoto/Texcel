"""Create a small deterministic OOXML fixture; no spreadsheet engine is used.

Explicit XML keeps empty strings, cached formula results and intentional errors
independent of an author's calculation engine. Output is not an Excel-produced file.
"""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED
import xml.etree.ElementTree as ET

MAIN = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
REL = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'

parts = {
    '[Content_Types].xml': '''<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>''',
    '_rels/.rels': f'''<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/>
</Relationships>''',
    'xl/workbook.xml': f'''<workbook xmlns="{MAIN}" xmlns:r="{REL}">
<workbookPr date1904="0"/>
<sheets><sheet name="売上" sheetId="1" r:id="rId1"/><sheet name="型と書式" sheetId="2" r:id="rId2"/></sheets>
<calcPr calcId="191029" fullCalcOnLoad="1"/>
</workbook>''',
    'xl/_rels/workbook.xml.rels': f'''<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="{REL}/worksheet" Target="worksheets/sheet2.xml"/>
<Relationship Id="rId3" Type="{REL}/styles" Target="styles.xml"/>
</Relationships>''',
    'xl/styles.xml': f'''<styleSheet xmlns="{MAIN}">
<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><color rgb="FFFFFFFF"/><name val="Calibri"/></font></fonts>
<fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FF245580"/><bgColor indexed="64"/></patternFill></fill></fills>
<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="4">
<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
<xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/>
<xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment wrapText="1"/></xf>
</cellXfs>
<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>''',
    'xl/worksheets/sheet1.xml': f'''<worksheet xmlns="{MAIN}">
<dimension ref="A1:C4"/>
<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>
<sheetFormatPr defaultRowHeight="15"/>
<cols><col min="1" max="3" width="18" customWidth="1"/></cols>
<sheetData>
<row r="1"><c r="A1" s="1" t="inlineStr"><is><t>単価</t></is></c><c r="B1" s="1" t="inlineStr"><is><t>数量</t></is></c><c r="C1" s="1" t="inlineStr"><is><t>金額</t></is></c></row>
<row r="2"><c r="A2"><v>1200</v></c><c r="B2"><v>3</v></c><c r="C2"><f>A2*B2</f><v>3600</v></c></row>
<row r="3"><c r="A3"><v>800</v></c><c r="B3"><v>2</v></c><c r="C3"><f>A3*B3</f><v>1600</v></c></row>
<row r="4"><c r="A4" t="inlineStr"><is><t>合計</t></is></c><c r="C4"><f>SUM(C2:C3)</f><v>5200</v></c></row>
</sheetData>
<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>
<pageSetup orientation="landscape" paperSize="9"/>
</worksheet>''',
    'xl/worksheets/sheet2.xml': f'''<worksheet xmlns="{MAIN}">
<dimension ref="A1:C8"/>
<sheetFormatPr defaultRowHeight="15"/>
<cols><col min="1" max="2" width="28" customWidth="1"/><col min="3" max="3" width="12" hidden="1" customWidth="1"/></cols>
<sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>00123</t></is></c><c r="B1"><v>123</v></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>12345678901234567890</t></is></c><c r="B2" t="b"><v>1</v></c></row>
<row r="3"><c r="A3" t="inlineStr"><is><t/></is></c><c r="B3"/></row>
<row r="4"><c r="A4" t="inlineStr"><is><r><rPr><b/></rPr><t>重要</t></r><r><t xml:space="preserve"> ABC</t></r></is></c><c r="B4" s="2"><v>45292</v></c></row>
<row r="5" ht="32" customHeight="1"><c r="A5" s="3" t="inlineStr"><is><t>日本語 ABC&#10;2行目</t></is></c><c r="B5" t="e"><f>1/0</f><v>#DIV/0!</v></c></row>
<row r="6"><c r="A6" t="inlineStr"><is><t>=SUM(A1:A2)</t></is></c></row>
<row r="7"><c r="A7" t="inlineStr"><is><t>結合</t></is></c></row>
<row r="8" hidden="1"><c r="A8" t="inlineStr"><is><t>非表示行</t></is></c></row>
</sheetData>
<mergeCells count="1"><mergeCell ref="A7:B7"/></mergeCells>
</worksheet>''',
}

if __name__ == '__main__':
    output = Path(__file__).with_name('workbook.xlsx')
    with ZipFile(output, 'w') as archive:
        for name, xml in sorted(parts.items()):
            ET.fromstring(xml)
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, '<?xml version="1.0" encoding="UTF-8"?>\n' + xml)
    print(f'Created {output.name}: {len(parts)} XML parts (not engine-validated)')
