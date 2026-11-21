"""Create a small XLSX fixture whose only reported loss is cell formatting."""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import xml.etree.ElementTree as ET

MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PACKAGE_REL = "http://schemas.openxmlformats.org/package/2006/relationships"

PARTS = {
    "[Content_Types].xml": '''<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>''',
    "_rels/.rels": f'''<Relationships xmlns="{PACKAGE_REL}">
<Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/>
</Relationships>''',
    "xl/workbook.xml": f'''<workbook xmlns="{MAIN}" xmlns:r="{REL}">
<sheets><sheet name="Style" sheetId="1" r:id="rId1"/></sheets>
</workbook>''',
    "xl/_rels/workbook.xml.rels": f'''<Relationships xmlns="{PACKAGE_REL}">
<Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="{REL}/styles" Target="styles.xml"/>
</Relationships>''',
    "xl/styles.xml": f'''<styleSheet xmlns="{MAIN}">
<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><sz val="18"/><name val="Calibri"/></font></fonts>
<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>
<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs>
<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>''',
    "xl/worksheets/sheet1.xml": f'''<worksheet xmlns="{MAIN}">
<sheetData><row r="1"><c r="A1" s="1" t="inlineStr"><is><t>Large title</t></is></c><c r="B1" t="inlineStr"><is><t>Plain value</t></is></c></row></sheetData>
</worksheet>''',
}


def main() -> None:
    output = Path(__file__).with_name("formatting-only.xlsx")
    with ZipFile(output, "w") as archive:
        for name, xml in sorted(PARTS.items()):
            ET.fromstring(xml)
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, '<?xml version="1.0" encoding="UTF-8"?>\n' + xml)
    print(f"Created {output.name}: formatting loss only")


if __name__ == "__main__":
    main()
