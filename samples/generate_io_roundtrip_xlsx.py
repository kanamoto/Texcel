"""Writes the minimal Japanese workbook used by the format round-trip check."""

from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


parts = {
    "[Content_Types].xml": """<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>""",
    "_rels/.rels": """<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>""",
    "xl/workbook.xml": """<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets>
</workbook>""",
    "xl/_rels/workbook.xml.rels": """<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>""",
    "xl/worksheets/sheet1.xml": """<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1:B2"/><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>項目</t></is></c><c r="B1" t="inlineStr"><is><t>値</t></is></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>りんご</t></is></c><c r="B2" t="inlineStr"><is><t>古い</t></is></c></row>
</sheetData></worksheet>""",
}


output = Path(__file__).with_name("io-roundtrip.xlsx")
with ZipFile(output, "w", ZIP_DEFLATED) as archive:
    for name, content in parts.items():
        archive.writestr(name, '<?xml version="1.0" encoding="UTF-8"?>\n' + content)
print(output)
