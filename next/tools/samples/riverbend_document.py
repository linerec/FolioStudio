"""Editable IDML parts for a facing-page publication, consumed by Folio importRun."""
import xml.etree.ElementTree as ET
import zipfile

NS = "http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging"
ET.register_namespace("idPkg", NS)
FACES = {"sans": ("NanumGothic", "Bold", "NanumGothicBold"),
         "serif": ("NanumMyeongjo", "Regular", "NanumMyeongjo")}
COLORS = {"Ink": "83 50 45 38", "Pine": "84 31 63 18", "Rust": "10 77 82 2",
          "Ochre": "2 32 80 0", "Mint": "29 8 35 0", "Cream": "3 4 14 0",
          "Black": "0 0 0 100", "Paper": "0 0 0 0"}
# face, size, leading, ink
STYLES = {
    "$ID/[No paragraph style]": ("serif", 11, 18, "Black"),
    "Masthead": ("sans", 47, 56, "Ink"), "Display": ("serif", 35, 45, "Ink"),
    "Title": ("sans", 23, 32, "Ink"), "Head": ("sans", 16, 23, "Ink"),
    "Deck": ("serif", 15, 23, "Ink"), "Body": ("serif", 11, 18, "Black"),
    "Drop": ("serif", 11, 18, "Black"), "Quote": ("serif", 20, 30, "Pine"),
    "Label": ("sans", 9, 13, "Black"), "Caption": ("sans", 8, 12, "Black"),
    "Listing": ("serif", 9.5, 15, "Black"), "Cell": ("sans", 10, 14, "Black"),
}


def node(parent, tag, **attrs):
    attrs = {k: str(v) for k, v in attrs.items()}
    return ET.SubElement(parent, tag, attrs) if parent is not None else ET.Element(tag, attrs)


def package(tag):
    return node(None, f"{{{NS}}}{tag}", DOMVersion="18.0")


def xml_zip(path, members, mime):
    with zipfile.ZipFile(path, "w") as out:
        for key, data in [("mimetype", mime.encode()), *sorted(members.items())]:
            info = zipfile.ZipInfo(key, (2026, 11, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED if key == "mimetype" else zipfile.ZIP_DEFLATED
            out.writestr(info, data)


class Issue:
    def __init__(self):
        self.parts = {}
        self.design = node(None, "Document", Self="d", Name="물굽이 서간", ActiveLayer="layer", DOMVersion="18.0", RGBProfile="uRGB", CMYKProfile="GRACoL2013_CRPC6.icc")
        self.parts["designmap.xml"] = self.design
        self.pages = {}
        self.count = 0
        self.stories = []
        self.resources()
        node(self.design, "Layer", Self="layer", Name="지면", Visible="true", Printable="true")
        self.make_spread("parent", [-2, -1], master=True)
        for n, group in enumerate([[1], *[[p,p+1] for p in range(2,20,2)], [20]]):
            self.make_spread(f"spread{n}", group)
        node(self.design, "Section", Self="section", PageStart="p1", Length=20, ContinueNumbering="false", PageNumberStart=1)
        for p in (-2, -1):
            self.text(p, 36, 21, 400, 16, "Label", "물굽이 서간   /   RIVERBEND LETTERS")
            self.box(p, 36, 749, 540, .7, "Pine")
            self.text(p, 36 if p == -2 else 519, 756, 57, 20, "Caption", "#PAGE", align="LeftAlign" if p == -2 else "RightAlign")
            self.text(p, 110, 756, 392, 20, "Caption", "가람솔시  ·  2026년 11월", align="CenterAlign")

    def uid(self, prefix):
        self.count += 1
        return prefix + str(self.count)

    def part(self, typ, name):
        part = package(typ)
        self.parts[name] = part
        node(self.design, f"{{{NS}}}{typ}", src=name)
        return part

    def resources(self):
        g = self.part("Graphic", "Resources/Graphic.xml")
        node(g, "Swatch", Self="Swatch/None", Name="None")
        for name, value in COLORS.items():
            node(g, "Color", Self="Color/"+name, Name=name, Model="Process", Space="CMYK", ColorValue=value)
        fonts = self.part("Fonts", "Resources/Fonts.xml")
        for key,(family,style,ps) in FACES.items():
            group = node(fonts, "FontFamily", Self="font"+key, Name=family)
            node(group, "Font", Self="face"+key, FontFamily=family, Name=family+" "+style,
                 PostScriptName=ps, FontStyleName=style, FullName=family+" "+style, FontType="TrueType", Status="Installed")
        res = self.part("Styles", "Resources/Styles.xml")
        pg = node(res, "RootParagraphStyleGroup", Self="paragraphs")
        for key,(face,size,leading,color) in STYLES.items():
            attrs = {"Justification":"LeftJustified", "DesiredLetterSpacing":0, "MinimumLetterSpacing":-2, "MaximumLetterSpacing":3} if key in ("Body","Drop","Listing") else {}
            if key == "Drop":
                attrs.update(DropCapLines=2, DropCapCharacters=1)
            style = node(pg, "ParagraphStyle", Self="ParagraphStyle/"+key, Name=key, PointSize=size,
                         FontStyle=FACES[face][1], FillColor="Color/"+color, Tracking=-8 if key=="Masthead" else 0,
                         Hyphenation="false", **attrs)
            prop = node(style,"Properties")
            node(prop,"AppliedFont",type="string").text = FACES[face][0]
            node(prop,"Leading",type="unit").text = str(leading)
        cg = node(res,"RootCharacterStyleGroup",Self="characters")
        node(cg,"CharacterStyle",Self="CharacterStyle/Accent",Name="강조",FillColor="Color/Black",Tracking=35)
        og = node(res,"RootObjectStyleGroup",Self="objects")
        node(og,"ObjectStyle",Self="ObjectStyle/Illustration",Name="그림 틀",StrokeWeight=0,FillColor="Swatch/None")
        tg = node(res,"RootTableStyleGroup",Self="tables")
        node(tg,"TableStyle",Self="TableStyle/Almanac",Name="생활표",TopBorderStrokeWeight=0,BottomBorderStrokeWeight=0)
        pref = self.part("Preferences","Resources/Preferences.xml")
        node(pref,"DocumentPreference",PageWidth=612,PageHeight=792,PagesPerDocument=20,FacingPages="true",
             DocumentBleedTopOffset=9,DocumentBleedBottomOffset=9,DocumentBleedInsideOrLeftOffset=9,
             DocumentBleedOutsideOrRightOffset=9,DocumentBleedUniformSize="true")
        node(pref,"GridPreference",BaselineStart=36,BaselineDivision=12,BaselineGridRelativeOption="TopOfPageOfBaselineGridRelativeOption")

    def make_spread(self, sid, pages, master=False):
        typ = "MasterSpread" if master else "Spread"
        part = self.part(typ, f"{typ}s/{typ}_{sid}.xml")
        opt = dict(Name="A-지면", NamePrefix="A", BaseName="지면") if master else {}
        spread = node(part,typ,Self=sid,PageCount=len(pages),BindingLocation=int(pages[0]%2==0),
                      ShowMasterItems="true",ItemTransform="1 0 0 1 0 0",**opt)
        for p in pages:
            dx = -612 if p%2 == 0 else 0
            page = node(spread,"Page",Self=f"p{p}",Name=str(p) if not master else "A",GeometricBounds="0 0 792 612",
                        ItemTransform=f"1 0 0 1 {dx} 0",AppliedMaster="n" if master or p in (1,20) else "parent")
            node(page,"MarginPreference",Top=36,Bottom=36,Left=36,Right=36,ColumnCount=3,ColumnGutter=13.5)
            self.pages[p] = spread,dx

    def shape(self, p, tag, x,y,w,h, **attrs):
        spread, dx = self.pages[p]
        frame = node(spread,tag, Self=self.uid("item"),ItemLayer="layer",
                     ItemTransform=f"1 0 0 1 {dx+x} {y}",StrokeWeight=0, **attrs)
        points = node(node(node(node(node(frame,"Properties"),"PathGeometry"),"GeometryPathType",PathOpen="false"),"PathPointArray"),"PathPointType",Anchor="0 0",LeftDirection="0 0",RightDirection="0 0")
        array = frame.find(".//PathPointArray")
        for corner in (f"{w} 0",f"{w} {h}",f"0 {h}"):
            node(array,"PathPointType",Anchor=corner,LeftDirection=corner,RightDirection=corner)
        return frame

    def box(self,p,x,y,w,h,color):
        return self.shape(p,"Rectangle",x,y,w,h,FillColor="Color/"+color,ContentType="Unassigned")

    def picture(self,p,x,y,w,h,name,wrap=False):
        f = self.shape(p,"Rectangle",x,y,w,h,ContentType="GraphicType",AppliedObjectStyle="ObjectStyle/Illustration",FillColor="Swatch/None")
        if wrap:
            pref = node(f,"TextWrapPreference",TextWrapMode="BoundingBoxTextWrap",TextWrapSide="BothSides")
            node(node(pref,"Properties"),"TextWrapOffset",type="rectangle").text="5 6 5 6"
        img = node(f,"SVG",Self=self.uid("svg"),ItemTransform=f"{w/600:.9f} 0 0 {h/440:.9f} 0 0")
        node(node(img,"Properties"),"GraphicBounds",Left=0,Top=0,Right=600,Bottom=440)
        node(img,"Link",Self=self.uid("link"),LinkResourceURI="Links/"+name+".svg",LinkResourceFormat="SVG")

    def story(self):
        sid = self.uid("story")
        self.stories.append(sid)
        s = node(self.part("Story",f"Stories/Story_{sid}.xml"),"Story",Self=sid)
        return sid,s

    def para(self,s,key,content,align=None,accent=False,grid=False,br=False):
        attrs = {}
        if align:
            attrs["Justification"]=align
        if grid:
            attrs["AlignToBaseline"]="true"
        para = node(s,"ParagraphStyleRange",AppliedParagraphStyle="ParagraphStyle/"+key,**attrs)
        run = node(para,"CharacterStyleRange", **({"AppliedCharacterStyle":"CharacterStyle/Accent"} if accent else {}))
        if content is not None:
            v = node(run,"Content")
            if content=="#PAGE":
                v.append(ET.ProcessingInstruction("ACE", "18"))
            else:
                v.text=content
        if br:
            node(run,"Br")
        return run

    def frame(self,p,x,y,w,h,sid,cols=1):
        f=self.shape(p,"TextFrame",x,y,w,h,ParentStory=sid,PreviousTextFrame="n",NextTextFrame="n",
                     ContentType="TextType",FillColor="Swatch/None")
        node(f,"TextFramePreference",TextColumnCount=cols,TextColumnGutter=13.5,VerticalJustification="TopAlign")
        return f

    def text(self,p,x,y,w,h,key,content,*,align=None,accent=False,grid=False,cols=1):
        sid,s=self.story()
        lines=[content] if isinstance(content,str) else content
        for i,line in enumerate(lines):
            self.para(s,key,line,align,accent,grid,br=i+1<len(lines))
        return self.frame(p,x,y,w,h,sid,cols)

    def article(self,p,x,y,w,h,paragraphs,drop=False):
        sid,s=self.story()
        for i,para in enumerate(paragraphs):
            run = self.para(s,"Drop" if drop and i==0 else "Body",para,br=i+1<len(paragraphs))
            if i + 1 == len(paragraphs) // 2:
                run.set("ParagraphBreakType", "NextFrame")
        gap=16
        first=self.frame(p,x,y,(w-gap)/2,h,sid)
        second=self.frame(p,x+(w+gap)/2,y,(w-gap)/2,h,sid)
        first.set("NextTextFrame",second.get("Self"))
        second.set("PreviousTextFrame",first.get("Self"))

    def table(self,p,x,y,width,rows,widths):
        sid,s=self.story()
        run=self.para(s,"Cell",None)
        t=node(run,"Table",Self=self.uid("table"),HeaderRowCount=1,FooterRowCount=0,BodyRowCount=len(rows)-1,
               ColumnCount=len(widths),AppliedTableStyle="TableStyle/Almanac")
        for i in range(len(rows)):
            node(t,"Row",Self=self.uid("row"),Name=i,SingleRowHeight=31,MinimumHeight=31)
        for i,col in enumerate(widths):
            node(t,"Column",Self=self.uid("col"),Name=i,SingleColumnWidth=col)
        for r,row in enumerate(rows):
            for c,value in enumerate(row):
                cell=node(t,"Cell",Self=self.uid("cell"),Name=f"{c}:{r}",FillColor="Color/"+("Mint" if r==0 else "Cream" if r%2 else "Paper"),
                          LeftInset=6,RightInset=4,TopInset=5,BottomInset=3)
                self.para(cell,"Cell",value)
        self.frame(p,x,y,width+2,len(rows)*35+25,sid)

    def write(self,path):
        self.design.set("StoryList"," ".join(self.stories))
        members = {k:ET.tostring(v,encoding="utf-8",xml_declaration=True) for k,v in self.parts.items()}
        members["META-INF/container.xml"]=b'<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="designmap.xml" media-type="text/xml"/></rootfiles></container>'
        xml_zip(path,members,"application/vnd.adobe.indesign-idml-package")
