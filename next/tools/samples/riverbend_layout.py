"""Page order, geometry, and editorial art direction of the November issue."""
from riverbend_copy import CLASSIFIEDS, ECON, EDITOR, EVENTS, FEATURE, FOOD, GUIDE, NEWS, SCHOOL
from riverbend_document import Issue


def heading(d,p,section,title,deck):
    d.text(p,36,62,540,20,"Label",section,accent=True)
    d.text(p,34,96,544,64,"Display",title)
    d.text(p,36,172,538,60,"Deck",deck)
    d.box(p,36,238,540,1,"Pine")


def entry(d,p,x,y,w,label,title,body):
    d.text(p,x,y,w,18,"Label",label,accent=True)
    d.text(p,x,y+22,w,52,"Head",title)
    d.text(p,x,y+78,w,132,"Body",body)


def composition():
    d=Issue()
    # Front cover
    d.box(1,-9,-9,630,810,"Cream")
    d.text(1,34,28,543,22,"Label","RIVERBEND LETTERS                                      가람솔 한인 생활 월간")
    d.box(1,36,56,540,1.5,"Ink")
    d.text(1,33,65,547,76,"Masthead","물굽이 서간")
    d.text(1,37,145,538,22,"Label","2026년 11월     /     창간호     /     VOL. 01",accent=True)
    d.picture(1,-9,180,630,441,"market")
    d.text(1,36,638,540,24,"Label","COVER STORY     ·     강변 시장의 하루")
    d.text(1,33,664,549,56,"Display","장이 서는 자리")
    d.box(1,36,728,540,1,"Pine")
    d.text(1,36,741,540,36,"Caption","두 작업실의 하루 값     /     귤 향이 밴 무 한 접시     /     소리로 그리는 동네 지도")

    heading(d,2,"CONTENTS   /   11","이달의 차례","강을 따라 걷고, 장을 보고, 집으로 돌아오는 한 달")
    d.picture(2,36,263,226,169,"river")
    d.text(2,36,445,220,52,"Caption","표지의 장소   /   가람솔시 물굽이장과 여울목 다리")
    toc=[("03","발행인 편지"),("04–07","특집 · 장이 서는 자리"),("08–09","동네에서 온 네 통의 소식"),
         ("10–11","생활 경제 · 하루의 값"),("12","부엌 · 귤 향이 밴 무"),("13","교실 · 소리를 그리다"),
         ("14–15","이달의 달력과 장보기 수첩"),("16–18","동네 가게 광고"),("19–20","안내 장터와 뒤표지")]
    for i,(n,title) in enumerate(toc):
        y=264+i*46
        d.text(2,285,y,58,25,"Label",n,accent=True)
        d.text(2,347,y-2,233,32,"Body",title)
        d.box(2,285,y+29,289,.55,"Mint")
    d.text(2,36,569,212,110,"Quote","장을 보러 갔다가 길 하나를 새로 배웠다.")

    heading(d,3,"LETTER   /   발행인 글","반대편으로 걷기","늘 보던 길에서 시작한, 가람솔의 첫 번째 편지")
    d.text(3,36,260,350,455,"Body",EDITOR)
    d.box(3,410,264,3,292,"Rust")
    d.text(3,431,272,144,187,"Quote","이 일을 누가 언제 다 했을까.")
    d.text(3,432,488,141,94,"Caption","글   /   모래온\n물굽이 서간 발행인".splitlines())
    d.text(3,36,707,530,24,"Label","2026년 늦가을, 물굽이장에서")

    for i,(title,paras) in enumerate(FEATURE):
        p=i+4
        if i%2==0:
            d.text(p,36,61,540,22,"Label",f"FIELD NOTES    /    특집 {i+1:02d}",accent=True)
            d.text(p,33,91,546,65,"Display",title)
            d.picture(p,36,166,540,236,["market","rain"][i//2])
            d.text(p,36,409,539,28,"Caption",["그림 1. 첫 수레가 들어온 뒤, 다리 밑으로 천막의 줄이 생긴다.","그림 3. 우산이 겹치는 자리마다 마른 길이 이어진다."][i//2])
            d.article(p,36,455,540,270,paras,drop=i==0)
        else:
            d.text(p,36,61,540,22,"Label",f"FIELD NOTES    /    특집 {i+1:02d}",accent=True)
            d.text(p,34,94,542,58,"Display",title)
            d.text(p,36,170,534,84,"Quote",["가게 둘이 서로의 문지기가 되는 시간은 채 삼 분이 되지 않는다.","천막을 접는 손은 벌써 다음 장날의 일을 하고 있었다."][i//2])
            d.article(p,36,274,540,270,paras)
            d.picture(p,36,554,270,160,["rain","river"][i//2])
            d.text(p,327,577,236,100,"Caption",["그림 2. 세 꼭짓점을 잇는 천 끝은 손수레가 다니는 통로의 경계다.","그림 4. 빈 수레는 여울목 다리에서 둑길로 돌아 나간다."][i//2])
    for i in range(2):
        p=8+i
        heading(d,p,"DISPATCHES   /   지역 소식",["모퉁이에서 온 편지","동네의 작은 실험"][i],
                ["길의 폭과 책 한 권이 바꾼 동네 풍경","같이 접고, 기다리고, 기록하는 사람들"][i])
        d.picture(p,36,260,540,161,["river","rain"][i])
        for col,(title,place,body) in enumerate(NEWS[i*2:i*2+2]):
            entry(d,p,36+col*279,452,259,place,title,body)
        d.text(p,36,704,540,32,"Caption","가람솔 통신원 공동 기록  ·  한 동네에서 일어난 네 가지 변화")

    heading(d,10,"LEDGER   /   생활 경제","작은 가게의 하루 값","월세 칸 옆에 문을 연 날을 함께 적어 보았다.")
    d.text(10,36,258,540,73,"Body",ECON[0])
    d.table(10,36,344,540,[["한 달 장부","둑길 작업실","골목 작업실"],["임차료","300솔","240솔"],
             ["운반비","60솔","96솔"],["영업일","18일","14일"],["하루 비용","20솔","24솔"]],[195,170,175])
    d.text(10,36,554,535,140,"Body",ECON[1:])
    d.text(10,36,708,540,22,"Caption","편집부의 가상 장부   /   하루 비용은 (임차료 + 운반비) ÷ 영업일로 셈했다.")

    heading(d,11,"FIGURES   /   숫자 읽기","낮은 월세의 다른 얼굴","같은 눈금 위에 놓은 두 작업실의 한 달 비용")
    for i,(title,rent,cart) in enumerate([("둑길",300,60),("골목",240,96)]):
        y=312+i*89
        d.text(11,36,y-24,160,24,"Head",title)
        d.box(11,36,y+10,rent*1.35,26,"Pine")
        d.box(11,36+rent*1.35,y+10,cart*1.35,26,"Ochre")
        d.text(11,36,y+42,539,20,"Caption",f"임차료 {rent}솔    운반비 {cart}솔    합계 {rent+cart}솔")
    d.box(11,36,515,20,11,"Pine")
    d.text(11,63,511,108,25,"Caption","임차료")
    d.box(11,159,515,20,11,"Ochre")
    d.text(11,186,511,172,25,"Caption","운반비   /   단위 솔")
    d.text(11,36,571,225,117,"Quote","문을 연 날도 비용의 일부다.")
    d.text(11,293,574,283,132,"Body","두 막대의 한 달 합계는 24솔 차이다. 그런데 문을 연 날까지 세면 하루 비용의 순서가 바뀐다. 자기 가게의 장부에도 실제로 불을 켠 날을 표시해 보면 어떨까.")

    heading(d,12,"PANTRY   /   계절 부엌","귤 향이 밴 무 한 접시","늦가을 물굽이장 초승찬방의 작은 냄비")
    d.picture(12,36,259,540,210,"pantry")
    d.text(12,36,477,539,28,"Caption","식탁 스케치   /   조리기구와 그날 들어온 과일을 함께 놓아 보았다.")
    d.article(12,36,523,540,201,FOOD)
    heading(d,13,"CLASSROOM   /   우리말 교실","지도를 귀로 그리면","구름턱 교실의 토요일 산책 수업")
    d.text(13,36,268,540,310,"Body",SCHOOL)
    d.picture(13,403,267,173,126,"river",wrap=True)
    d.text(13,296,552,280,77,"Deck","소리 셋, 종이 한 장, 연필 한 자루.")
    d.box(13,36,644,540,1,"Pine")
    d.text(13,36,662,540,64,"Body","집에서 해 볼 일   /   소리 기호 하나를 가족에게 건네고 어느 길인지 맞혀 본다.",grid=True)

    heading(d,14,"ALMANAC   /   행사 달력","11월, 만나는 날","가람솔시의 여섯 가지 작은 약속")
    d.table(14,36,268,540,EVENTS,[115,150,275])
    d.text(14,36,545,540,48,"Head","참가 수첩")
    d.text(14,36,607,526,110,"Body","씨앗 모임에는 빈 종이봉투, 주머니 짓기에는 집에 있는 헌 우산천 한 조각을 챙긴다. 도구는 둘이 나눠 쓴다. 야외 모임 날에 비가 오면 같은 시각에 구름턱 교실 창가에서 모인다.")
    heading(d,15,"POCKET GUIDE   /   생활 정보","장 보러 가는 길","처음 물굽이장을 찾는 이에게 건네는 수첩")
    for i,(title,body) in enumerate(GUIDE):
        y=270+i*132
        d.text(15,36,y,116,46,"Title",f"0{i+1}")
        d.text(15,157,y,420,37,"Head",title)
        d.text(15,157,y+42,419,81,"Body",body)
        d.box(15,36,y+112,540,.7,"Mint")

    # Seven interior ads and a back-cover ad. Each has its own editable copy.
    d.box(16,-9,44,630,693,"Mint")
    d.text(16,36,68,540,25,"Label","동네 광고   /   바큇살 공방")
    d.text(16,34,119,545,72,"Display","한 바퀴 더, 가뿐하게")
    d.picture(16,36,210,540,345,"cycle")
    d.text(16,36,582,540,64,"Title","바큇살 공방")
    d.text(16,36,648,530,66,"Body","돌아가는 소리를 함께 듣고 낡은 부품을 살핍니다. 토요일 첫 장이 서면 여울목 다리 아래에서 만나요.")
    ads=[("초승찬방","냄비째 나누는 저녁","무와 귤을 졸인 반찬을 빈 통에 담아 가세요. 장이 서는 토요일, 천막 안쪽 작은 식탁에서.","pantry"),
         ("여울천 우산방","비를 한 번 더 맞도록","휘어진 우산살 한 곳을 살피고 남은 천으로 손잡이 끈을 지어 드립니다.","rain")]
    for i,(name,title,body,art) in enumerate(ads):
        y=58+i*335
        d.box(17,36,y,540,310,["Cream","Mint"][i])
        d.text(17,51,y+15,495,20,"Label","동네 광고   /   "+name,accent=True)
        d.picture(17,52,y+59,223,178,art)
        d.text(17,294,y+67,264,78,"Title",title)
        d.text(17,295,y+159,253,132,"Body",body)
    cards=[("종이달 제본소","빈 장을 묶다","얇은 공책도 손에 맞는 폭이 있습니다. 자투리 종이를 골라 내 장보기 수첩을 만드세요.","Cream"),
           ("연둣결 씨앗집","한 줌의 내년","마른 꼬투리에서 골라낸 씨앗을 작은 봉투에 나눠 담습니다. 화분 크기를 알려 주세요.","Mint"),
           ("두굽 나무방","다시 선 의자","수평이 맞지 않는 의자와 작은 수레의 나무 바퀴를 살핍니다. 흠집 난 자리는 남겨 둡니다.","Mint"),
           ("물빛 그릇방","국물의 자리","찬물에 먼저 헹궈 쓰는 작은 국그릇. 장을 마치는 시간에는 쓴 그릇을 함께 씻습니다.","Cream")]
    for i,(name,title,body,color) in enumerate(cards):
        x=36+(i%2)*278
        y=60+(i//2)*335
        d.box(18,x,y,262,310,color)
        d.text(18,x+16,y+19,226,24,"Label","광고   /   "+name)
        for j in range(5):
            d.box(18,x+17+j*17,y+61+j*3,12,36-j*5,["Rust","Pine","Ochre"][i%3])
        d.text(18,x+15,y+117,237,48,"Title",title)
        d.text(18,x+16,y+178,228,113,"Body",body)

    heading(d,19,"NOTICEBOARD   /   안내 광고","작은 장터","빌리고 나누고 찾는 아홉 장의 쪽지")
    for i,(tag,title,body) in enumerate(CLASSIFIEDS):
        x=36+(i%3)*184.5
        y=260+(i//3)*155
        d.text(19,x,y,170,19,"Caption",tag,accent=True)
        d.text(19,x,y+22,170,35,"Head",title)
        d.text(19,x,y+60,169,89,"Listing",body)
        d.box(19,x,y+146,170,.6,"Mint")
    d.box(20,-9,-9,630,810,"Cream")
    d.text(20,36,34,540,22,"Label","전면 광고    /    가람솔 물길문고",accent=True)
    d.text(20,34,88,542,70,"Display","돌아가는 길에, 한 장")
    d.text(20,36,161,535,64,"Deck","우리가 걸은 만큼 지도는 새로 그려집니다.")
    d.picture(20,-9,250,630,395,"river")
    d.text(20,36,673,535,44,"Title","물길문고")
    d.text(20,36,734,540,32,"Caption","여울목 다리 너머 작은 책방   ·   토요일에는 저녁 장이 끝날 때까지")
    return d
