Bölüm 3 · Atarikafa Dungeons · ZindanCraft S4

# ZindanCraft çekim planı

Modun kodunu, veri dosyalarını ve dil dosyasını okudum. Bölüm 1 ve 2'de çıkan kalıpları bu modun içeriğine uyguladım: seri mi tek video mu, nasıl kaydedilir, yüz kamerası olmadan nasıl tutulur, ilk 90 saniye ve ilk 5 dakika nasıl kurulur.

Kısa cevap

## Sezon olarak çek: 6 ana bölüm, her biri tek başına izlenir. Sonunda tek parça "Tüm hikaye".

1. **8 saat kayıt edip tek videoya basma.** Modun yarısından fazlası ortada kalır ve ilk 5 dakikada tutma işi (Bölüm 2) tek dev videoda çok zorlaşır.
2. **İki gün oynayıp tek video yapma.** Bu mod "tek günlük" içerik değil: 7 zindan teması, 28 boss, 5 tier, 6 sınıf ve Abyss var. Ama tek videoyla **test etmek** mantıklı: ilk bölümü bağımsız bir pilot gibi çek.
3. **Mod zaten bir sezon iskeleti veriyor.** Tier 1'den Tier 5'e, oradan Taç'a ve Abyss'e uzanan bir ilerleme zinciri var. Her anahtar bir bölüm olur.
4. **Yüz kamerası olmadan tutar.** Raporlardaki videoların önemli kısmı kamerasız. Yerine kelime kelime altyazı, sayaç, tepki sesi ve modun kendi başlık kartları var.

**Ne yaptım, ne yapmadım:** Kodu, kaynak dosyaları ve dil dosyasını okudum. Modu oyunda çalıştırmadım ve ekran görüntüsü yok (profilin screenshots klasörü boş), o yüzden bir şeyin ekranda nasıl göründüğünü söyleyemem. Bölüm akışını koddan çıkardım, oyunda doğrulanmadı. Kanalının eski izlenme verisi bende yok, öneriler Bölüm 1 ve 2'deki 60+30 video ve senin anlattıkların üzerine.

Mod

## Ne var içinde

**7**zindan teması: crypt, deep, frost, jungle, mesa, mire, tomb

**92**yaratık, 28'i boss (her temada 4 koruyucu)

**6**sınıf, her birinde 10 yetenek

**5**tier + Abyss boyutu

**3**krallık boyutu: 64, 200, 480 kişi

**312**ses kaydı (toplam 411 ses)

**\~190**eşya: silah, şapka, yüzük, çanta, set zırh

**248**dekor bloğu (hanlar, kalelere kadar)

Rank 0**Tier 1**Bir zindanın ilk katları. Sınıf seç, haritayı al, boss'u devir. Boss bir anahtar bırakır.

Rank 1**Tier 2**Anahtarı krallıktaki lonca memuruna göster, sonraki katlar açılır.

Rank 2**Tier 3**Boss'lar daha çok yetenek kullanır (çekme, alan atağı).

Rank 3**Tier 4**Ateş halkası boss yeteneği. Ödüller Tier 4 sandıklarından.

Rank 4**Tier 5**En derin katlar. Boss dört yeteneğin hepsini kullanır. En güçlü silahlar (Abyssal Edge gibi) Tier 5 sandıklarından.

Rank 5**Taç ve Abyss**"Son Kralın Tacı"nı büyük krallıkta göster, Dungeon Master ol. Abyss açılır: her derinlikte +%20 can, +%12 hasar.

Her zindan 5–8 kat (temaya göre) ve Tier 1'den Tier 5'e doğru iniyor. Tier sınırlarında kilit var: "Sealed – take a Tier N key to a guild clerk first". Kaynak: `BiomeDungeonLayout`, `DungeoneerRank`, `Abyss`, dil dosyası.

Sınıflar

### 6 sınıf, 6 farklı bölüm stili

- **Warden**: tank, en çok can ve zırh, yavaş öldürür.
- **Reaver**: en sert ve hızlı vuruş, az savunma.
- **Stalker**: gizlilik ve yay, fark edilmemiş düşmana ağır ilk vuruş.
- **Wayfarer**: en hızlı, dash, blink ve çift zıplama.
- **Bloodsworn**: vampir, en az can, vurdukça iyileşir.
- **Seeker**: şans, menzil, kazı hızı, düz dövüşte en zayıf.

Dünya

### Zindan dışında yapılacak çok şey

- Yollar, işaret levhaları, waystone'lar, gezgin tüccarlar ve vagonlar.
- Krallıklar: lonca, ahır, büyük demirhane, gece kapanan kapılar, 7 günde bir pazar günü.
- Haydut kampları ve esirler (esir kurtarma).
- Limanlar, binilebilen gemiler (top, yelken), korsan adaları ve "Korsan Haritası".
- 22 yol kenarı yapısı, her birinde konuşan sakin ve takas.

Kayıp

### Ölüm ve sonuç mekanikleri

- **Ruh taşı**: öldüğün yerde yükselir, tüm eşyanı 54 yuvada tutar, koordinatı sohbete yazar.
- **Zindan notu**: bitirince "Cleared · Grade S/A/B", ölüm sayısı ve "cursed" yazar.
- Düşen maceracılar, rakip gruplar, tuzaklı koridorlar, lanetli sigil.

Kamera için

## Modun hazır verdiği 8 ekran anı

Bölüm 1 ve 2'de başkalarının elle yaptığı şeylerin çoğunu bu mod kendiliğinden veriyor. Bunları bilerek kaydet ve edit'te kullan.

| Mod anı | Ekranda ne olur | Raporlardan karşılığı |
| --- | --- | --- |
| Ruh taşı | Ölünce eşyaların taşa dönüşür, sohbette koordinat. Geri dönüp alırsın. | "Öldün!" ekranı bir ritim aracıydı (Bölüm 2). Burada kaybın bedeli de var: gerilim yükselir. |
| Zindan notu | "Cleared · Grade S · 2 deaths · cursed" | Bölüm sonu sahnesi. Sonuç kartı, thumbnail ve açılış teaser'ı olur. |
| Boss çubuğu | Boss uyanır, sarsıntı sesi, "X is enraged!" yazısı, halka ve çekme yetenekleri | Teaser montajının ana sahnesi (Bölüm 1, 5–15. saniye). |
| Giriş bandı | "The Hollow Crypt · Tier 1 · Level 1–5" gibi yazı | Hazır bölüm kartı. "CÜCELER KÖYÜ" gibi kartların aynısı (Bölüm 2). |
| Eşya açıklaması | Her silahın özelliği: "Her 5. vuruş yıldırım çağırır" | Mor tooltip kutusu, 2 saniye yakın çekim (Bölüm 2, döngü adım 2). |
| Takvim ve sayaç | HUD'da takvim, sezon günleri, rank, Abyss derinliği | "GÜN 1 → GÜN 6", sayaçlar (Bölüm 1 ve 2). |
| Yeteneklerin ve sınıfın seçimi | Sınıf seçme ekranı, kit, beceri ağacı | "Yeni Tarifler Açıldı" bildirimleri gibi ilerleme dopamini. |
| Gemi ve top | Dümen, yelken kademeleri, top yükleme, "Pirate Hideout" bandı | Monotonluğu kıran dış mekan bölümü. Teaser'a uygun. |

Karar

## Seri mi, tek video mu, 8 saatlik kayıt mı

Dört seçeneği yan yana koydum. Puanlar (●) benim yargım, ölçüm değil. Kanıt kısmı Bölüm 1'deki 60 videonun verisi.

| Seçenek | Modu gösterir | İzleyiciyi tutar | Çekim + edit yükü | Risk |
| --- | --- | --- | --- | --- |
| A · 8 saat kayıt, tek video | ●●●*●●* | ●*●●●●* | ●*●●●●* (çok ağır) | Dağınık, 90. saniyeden sonra düşer. Mod yarım kalır. |
| B · 1–2 gün, tek video | ●*●●●●* | ●●●*●●* | ●●●●*●* (hafif) | Büyük modun %80'i boşa gider, tekrar çekemezsin. |
| C · Sezon: 6 ana bölüm + 2 yan + "Tüm hikaye" | ●●●●● | ●●●●*●* | ●●●*●●* (orta) | Bölüm sayısı büyür. Pilot ile test ederek azaltılır. |
| D · 15–20 bölümlük klasik seri | ●●●●● | ●●●*●●* | ●*●●●●* (çok ağır) | Son yıllarda azaldı demiştin. Tükenme riski yüksek. |

### Süre kutusuna göre medyan izlenme (Bölüm 1'deki 60 video)

Kutu başına video sayısı alt etikette (n). Büyük kanallar ve küçük kanallar karışık, sonuç kaba bir işarettir. 10–15 dk yüksek sayıda ens videosundan oluşuyor, 35+ dk kutusu çoğunlukla uzun hikaye videoları.

### Uzun tek parça videolar nasıl çalışıyor

Hepsinde tek bir söz var ve baştan sona bir hikaye akıyor. YusufTe'nin videosu, bir yıllık serisinin tek bölümlük sürümü (kendi anlatımıyla):

- YusufTe tek blok, 101 dk, 2,3M [▶ aç](https://www.youtube.com/watch?v=UGYJB76IH30)
- Algomi tek blok, 104 dk, 1,1M [▶ aç](https://www.youtube.com/watch?v=OS_Pjox41aQ)
- Wity parkur medeniyeti, 74 dk, 1,1M [▶ aç](https://www.youtube.com/watch?v=PZ4IROpV_cY)

### Orta uzunlukta "gün sayaçlı" macera

Bir sayaç ve bir güçlenme hikayesiyle 35–40 dakikada milyonlara çıkanlar var. Zindan modundaki rank ve tier, bu sayacın karşılığı:

- Çimen Adam, chunk'tan çıkmadan 100 gün, 37 dk, 2,2M [▶ aç](https://www.youtube.com/watch?v=2I3mV1ihADM)
- Çimen Adam, tek şans bloğunda 100 gün, 36 dk, 969K [▶ aç](https://www.youtube.com/watch?v=mh1fWgasq3A)
- MiracBt güneş kıyametinde 100 gün, 21 dk, 1,2M [▶ aç](https://www.youtube.com/watch?v=uUI5dqkSjiU)

Plan

## Önerilen sezon

Her bölüm tek başına bir söz veriyor ve bir sonuçla bitiyor. Bölüm süreleri 15–25 dakika. Aşağıdaki akış koddan çıkarıldı, oyunda sırasını denemeden kesinleştirme.

1pilot

### Sıfırdan ilk zindan: sınıf seç, Tier 1 boss'u devir

Hoş geldin mesajı, sınıf seçimi (G), kit, haritayı al, ilk zindan, ilk boss, zindan notu. Tek başına izlenebilir, çünkü ödülü var: bir anahtar. Bu bölüm sezonun testi.

15–20 dkilk ruh taşıanahtar #1

2rank 1

### Krallığa git, loncaya anahtarı göster, Tier 2'ye in

Krallık gezisi, beceri ağacı, ilk ciddi eşya. Loncadaki anahtar sahnesi bu bölümün "açılış" sahnesi.

krallıkbeceri puanıanahtar #2

3yan

### Haydut kampı ve esir kurtarma

Zindan dışı, 12–15 dakika. Ritmi bozar ve monotonluğu kırar. Esir kurtarma sonuçlu bir mini hikaye.

12–15 dkisteğe bağlı

4rank 2–3

### Tier 3 ve 4: boss yetenekleri artıyor

Çekme ve ateş halkası yetenekleri sahneye çıkıyor. Zırh setleri (Ashen, Kingsguard, Shadewalker, Warden) ve güçlü silahlar için ilk büyük sahne.

boss yeteneklerianahtar #3 ve #4

5yan

### Gemiyi al, korsan adasına git

Korsan Haritası, yelken kademeleri, top. Sezonun görsel olarak en farklı bölümü ve Shorts için malzeme.

12–15 dkisteğe bağlı

6rank 5

### Tier 5 ve Son Kralın Tacı: Dungeon Master

En derin kat, son boss, tacı büyük krallığın loncasına götürme. Sezonun finali ve Abyss'in kapısı.

final bosstaç

7final

### Abyss: kaç derinliğe inebilirsin?

Her derinlikte +%20 can ve +%12 hasar ve sayaç doğal olarak artıyor. Bu, "100 gün" tarzı sayaç videosunun zindan karşılığı.

Abyssderinlik sayacı

8bonus

### TÜM HİKAYE: sezonun 60–90 dakikalık filmi

Bölümlerin kısaltılmış, tek hikaye gibi akan sürümü. Yukarıdaki üç uzun örnek bunun işe yaradığını gösteriyor.

60–90 dksezon bitince

**Pilot kuralı:** Önce sadece 1. bölümü çek ve çıkar. İlk 24 saatte kapak/başlık ve ilk 30 saniye tutma grafiğine bak. Son birkaç videonun medyanından belirgin düşükse sezona geçmeden önce kapağı, başlığı ve hook'u değiştir. Bu karar kuralı benim önerim, bir eşiği ölçerek doğrulanmış değil.

Yüz kamerası

## Kameran yok: yerine ne koyacaksın

Bölüm 1 ve 2'de kamerasız ama başarılı videolar var. Hepsi aynı beş şeyi kullanıyor.

### Kelime kelime altyazı

1–3 kelime, kalın kenarlık, alt orta. Wollech'in videolarında en sık görülen araç [▶ 0:04](https://www.youtube.com/watch?v=ZoAPnoJNaNE&t=4s). Modda İngilizce yazılar olduğu için önemli: oyun yazısı yerine kendi Türkçe altyazın ekranda en büyük yazı olsun.

### Sayaç ve kartlar

Rank, tier ve Abyss derinliği için sağ üstte sayaç. Antarktika videosundaki "GÜN" sayacı gibi [▶ 0:30](https://www.youtube.com/watch?v=UC-KVNeJ24M&t=30s). Bölüm kartı olarak modun kendi giriş bandını kullan.

### Ses tepkisi

Kamera yoksa tepki sesinde saklı. "Oha", kahkaha, bağırma, sessiz kalma. Mikrofonu ayrı kanalda kaydet, oyun sesini kısma: modun boss müziği ve ses kayıtları sahnenin yarısı.

### Maskot veya logo animasyonu

Wollech çizgi film karakteriyle başlıyor [▶ 0:04](https://www.youtube.com/watch?v=ZoAPnoJNaNE&t=4s). Modun adı kanalın adı: "Atarikafa Dungeons". Kısa bir logo giriş animasyonu ücretsiz marka olur.

### Tepki zoom'u ve gri çerçeve

Boss uyandığında hafif zoom, ölümde kırmızı ekran. Gri küçük çerçeve geri dönüş için: [▶ 0:55](https://www.youtube.com/watch?v=eoEAf3TP5aY&t=55s).

### Kaçın

- Sessiz kalan uzun anlar.
- Altyazısız İngilizce menü kalabalığı: küçük ekranda okunmaz.
- Açılışta uzun kurulum konuşması.

Uygulama

## 1. bölümün ilk 90 saniyesi ve ilk 5 dakikası

Bölüm 1'deki 5 parçalı açılış ve Bölüm 2'deki döngü, bu modun içeriğiyle yazıldı. Cümleler taslak, kendi dilinde yaz.

| Süre | Ekranda | Söylenen | Edit |
| --- | --- | --- | --- |
| 0:00–0:04 | Boss'un uyanışı veya Tier 5 yıldırım vuruşu (önceden çek) | "Minecraft'ı RPG yaptım: sınıf seçiyorum, zindana iniyorum, ölürsem her şeyimi kaybediyorum." | "MINECRAFT AMA" tarzı kısa logo kartı + kelime kelime altyazı |
| 0:04–0:14 | Teaser: boss enrage, ruh taşı, gemi topu, zindan notu "Grade S" | "Peki boss uyanıp yere ateş halkası yollarsa ne olur?" | Her kesit 1–2 sn, ses efekti, en güçlü sahne sonda |
| 0:14–0:20 | Hoş geldin mesajı, sınıf seçimi ekranı açılıyor | Tek cümle abone çağrısı. "Önce sınıfımı seçiyorum." | Abone kartı 3 sn, logo animasyonu |
| 0:20–0:45 | Sınıfın kiti ve haritası | Sınıf sloganını Türkçe oku ("Her yara ödenmiş bir borçtur") | Tooltip yakın çekim, Türkçe alt yazı bandı |
| 0:45–1:30 | İlk yürüyüş, yol işaretleri, zindan girişi ("The Hollow Crypt" bandı) | "Bu kapının altında ne var bilmiyorum." | Modun giriş bandı = bölüm kartı. İkinci abone çağrısı ≈1:25 |
| 1:30–2:30 | İlk kat, ilk mob, ilk tuzaklı koridor | İlk gerçek tepki | Eşya açıklaması 2 sn, sonra denemeye kes |
| 2:30–3:30 | İlk risk: elit mob veya düşmüş maceracı, muhtemel ilk ölüm | Ölürsen: "Her şey ruh taşında, geri gitmem lazım." | Ölüm ekranını kes, koordinat satırını göster |
| 3:30–5:00 | Ruh taşını geri almak, ilk sandık, bir sonraki kata iniş | "Her kat daha derin, boss en altta." | Kat sayacı "Level 2 of 6 · Tier 1" bandı, sponsor varsa burada |

Kayıt rehberi

## Nasıl kaydedersin

### Ayarlar

- 1080p60 kaydet. Profilinde GUI ölçeği zaten 3, 360p'de menüler okunur kalır. Bunu değiştirme.
- Profilde JEI, Xaero haritası ve AppleSkin var. Çekimlerde JEI paneli ve harita kutusu gizli olsun, ekranı kalabalıklaştırıyorlar.
- Mikrofon ayrı kanal. Oyun sesi %60–70.
- İlk oyunda F1 ile temiz sahnelik çekimler al: boss girişleri, gemi, krallık.

### Her bölüm için

- Önce teaser sahnelerini ayrıca çek (boss enrage, yıldırım, ruh taşı). Gerekirse mod komutlarıyla hazırla: `/dungeoneer set`, `rank`, `abyss` (op yetkisi gerekebilir; denemedim).
- Sonra bölümü baştan sona kaydet, ham süre 90–150 dakika, final 15–25. Bu oran benim tahminim.
- Her ölümü ve ruh taşı geri alışını kaydet. Kesersin, ama gerginlik orada.
- Bölüm sonunda zindan notunu 3 saniye sabit göster.

### Dil

Mod sadece İngilizce (en_us, yaklaşık 4.800 anahtar). Hepsini çevirmen gerekmez: sınıf adları ve sloganları, boss isimleri, giriş bantları ve hoş geldin mesajı yeter, yaklaşık 150 satır. İstersen bir `tr_tr.json` dosyası için ben çeviri taslağı hazırlayabilirim.

Başlık ve kapak

## Önerilen başlıklar ve kapaklar

Bölüm 1'deki kalıp: büyük harfli 1–2 kelime, rakam, "!" veya "...". Her başlık bir söz ve sonucun merakını taşıyor.

Minecraft'ı *RPG* Yaptım: 28 BOSS, 6 SINIF, 7 ZİNDAN

Kendi Yaptığım *ZİNDAN MODUNDA* Hayatta Kalmaya Çalıştım...

Minecraft'ta *ÖLÜNCE* Her Şeyimi Kaybediyorum! (Zindan Modu)

Minecraft'ta *ZİNDAN USTASI* Olmaya Çalıştım (TÜM HİKAYE)

Minecraft'ta *ABYSS*'in En Derinine İndim...

Minecraft Zindan Modu: *TIER 5* Boss'u Yendim mi?

### Kapak fikirleri

- Boss'un adı ve "TIER 5" rakamı.
- Zindan notu kartı ("Grade S") büyük, tek başına.
- Silah tooltip'i ("Her 5. vuruş yıldırım çağırır"): tooltip kutuları kapakta okunuyor.
- Ruh taşı + "KAYBETTİM" yazısı.

### Marka

Modun adı "Atarikafa Dungeons", kanalın adı atarikafa. Başlıkta veya kartta kullan: hem mod sayfasına hem kanala trafik taşır.

Çekimden önce

## Roadmap'teki, kamerada görünecek eksikler

Mod 0.1.0 ve kendi notlarında "derin oyun testi" bekliyor. Kamerada en çok iz bırakacak maddeler:

| Madde (ROADMAP.md) | Neden önemli |
| --- | --- |
| Zırh ve silahlar vanilyaya çok yakın | Oyuncu zindanı bırakıp elmas/netherite yapabilir. Ödül sahneleri anlamsızlaşır. İlk bölümden önce çöz. |
| Efsanevi silah ve zırh yok | Sezonun finali için büyük ödül lazım. |
| Boss minyonları hâlâ vanilya | Teaser sahnesinde fark edilir. |
| Zindan, boss, krallık, gemi, GUI ve ses testi yapılmadı | Çekimde hata çıkarsa o sahne yeniden çekilir. Önce 1–2 saat testle bölümleri dene. |
| Mod ikonu yok | Modrinth sayfası ve açıklama linki için küçük ama görünür bir eksik. |

Kaynak: `C:\www\atarikafa-dungeons` (README, docs, creatures/abilities/buildings json, en_us.json, BossController, Abyss, DungeoneerRank, BiomeDungeonLayout, config), ZindanCraft S4 profil klasörü (mod listesi, options.txt). Önceki raporlar: [Bölüm 1](https://claude.ai/artifact/JSbRXbfxft1X9y8d564REz) · [Bölüm 2](https://claude.ai/artifact/MsjeeGjYgv6MJhjkm7LLqi)