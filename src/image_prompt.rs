//! Shared cover-image prompt builder used by every image-generation backend.
//!
//! Produces a single Chinese prompt asking for an editorial-quality 2D
//! illustration in the spirit of independent tech magazines (Increment,
//! Stripe Press), suitable for senior software engineers with taste.
//! Keeping every backend on the same prompt template means the aesthetic
//! does not drift when a new provider is added.

const MAX_CONTENT_LEN: usize = 2000;

/// Builds the cover-image prompt from raw markdown content.
pub fn build_cover_prompt(content: &str) -> String {
    let truncated = truncate_at_char_boundary(content, MAX_CONTENT_LEN);

    format!(
        "为微信公众号技术文章生成一张封面插图，宽幅 16:9。\n\
         读者是有审美的资深软件工程师；图像应当像独立技术杂志（Increment / Stripe Press / 纽约客特稿插图 一类）的特稿插图，\
         看起来像出自一位有思想的插画师之手，而不是 AI 生成的科技素材。\n\n\
         文章内容：\n\
         {}\n\n\
         请先识别文章的具体主题（例如「钓鱼邮件 / 邮件安全」「分布式一致性」「编译器优化」——是哪一个），\
         然后构思一个让读者在缩略图尺寸下也能迅速读懂这个主题的概念画面。\n\n\
         画面构思：\n\
         - 主题必须一眼可读：通过物件的关系（位置、姿态、对比、缺失、错位、入侵、伪装）传达文章的核心张力\n\
         - 例如：讲「信任的渠道被穿透」→ 一封信封从墙缝渗出 / 落在墙的错误一侧 / 一只手从信封内部把另一只手拉进去；\
         讲「系统的脆弱平衡」→ 一个被轻微扰动的几何结构。概念可以诗意，但题材必须具体、可识别\n\
         - 通过物件、几何结构或建筑空间承载隐喻；不要用拟人化角色或卡通表演来讲故事\n\
         - 大面积留白；构图可以非中心；主体克制\n\n\
         风格与质感：\n\
         - 严格二维插画 / 杂志特稿插图的工艺感；从下列媒介中选一个最契合主题的：\
         gouache 水粉、版画 / 丝网印 / risograph、钢笔淡彩、淡墨水彩、isometric 平面示意图、安静的 matte 数字绘画\n\
         - 色彩克制典雅：最多 3 种主色，从 墨色 / 米白 / 黛青 / 赭石 / 靛蓝 / 陶土 / 橄榄 / 灰玫 这一沉静色系中挑选\n\
         - 能感受到笔触、纸纹、墨色叠印或印刷颗粒；避免光滑塑料感\n\n\
         严格禁止：\n\
         - 任何形式的 3D 渲染、3D 建模、3D 角色、Pixar / Disney 风、塑料材质感、CGI 影视特效感\n\
         - 任何拟人化的卡通人物、反派形象、吉祥物、动物代言、蒙面角色\n\
         - 霓虹色、电光蓝、紫色辉光、高饱和荧光色\n\
         - 霓虹边光、发光粒子、体积光光柱、镜头光晕、赛博朋克氛围\n\
         - 任何文字、字符、商标、logo、水印、品牌标识",
        truncated
    )
}

fn truncate_at_char_boundary(content: &str, max_len: usize) -> &str {
    if content.len() <= max_len {
        return content;
    }
    let mut end = max_len;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    &content[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cover_prompt_includes_content_and_aspect() {
        let prompt = build_cover_prompt("分布式系统的最终一致性");
        assert!(prompt.contains("分布式系统的最终一致性"));
        assert!(prompt.contains("16:9"));
    }

    #[test]
    fn build_cover_prompt_forbids_three_d_and_neon() {
        let prompt = build_cover_prompt("test");
        assert!(prompt.contains("3D"));
        assert!(prompt.contains("霓虹"));
        assert!(prompt.contains("严格禁止"));
    }

    #[test]
    fn truncate_handles_multibyte_boundary() {
        let content: String = "中".repeat(1000); // 3000 bytes UTF-8
        let truncated = truncate_at_char_boundary(&content, 2000);
        assert!(content.is_char_boundary(truncated.len()));
        assert!(truncated.len() <= 2000);
    }

    #[test]
    fn truncate_passes_short_content_through() {
        let truncated = truncate_at_char_boundary("hello", 2000);
        assert_eq!(truncated, "hello");
    }
}
