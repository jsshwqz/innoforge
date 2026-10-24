//! 单元测试共享夹具（cfg(test) only；集成测试自带同款夹具）。

use crate::evidence_kernel::model::*;

pub fn fixture_corpus() -> SourceCorpus {
    SourceCorpus {
        docs: vec![
            SourceDoc {
                doc_id: "D1".into(),
                kind: "patent".into(),
                title: "对比文件1（温控风扇）".into(),
                sections: vec![
                    SourceSection {
                        section_id: "D1:para:35".into(),
                        kind: LocatorKind::Paragraph,
                        number: 35,
                        text: "当温度超过阈值时，控制器自动启动风扇并根据温差调节转速。".into(),
                    },
                    SourceSection {
                        section_id: "D1:claim:2".into(),
                        kind: LocatorKind::Claim,
                        number: 2,
                        text: "根据权利要求1所述的装置，其特征在于，风扇转速可自动调节。".into(),
                    },
                ],
            },
            SourceDoc {
                doc_id: "D2".into(),
                kind: "patent".into(),
                title: "对比文件2（相变储热）".into(),
                sections: vec![SourceSection {
                    section_id: "D2:para:12".into(),
                    kind: LocatorKind::Paragraph,
                    number: 12,
                    text: "相变储热模块在温度上升时吸收热量，抑制峰值温度。".into(),
                }],
            },
            SourceDoc {
                doc_id: "引用1".into(),
                kind: "patent_chunk:description".into(),
                title: "本申请说明书切片".into(),
                sections: vec![SourceSection {
                    section_id: "引用1:chunk:0".into(),
                    kind: LocatorKind::Chunk,
                    number: 0,
                    text: "散热底座与均热板之间设置有导热界面材料，用于降低接触热阻。".into(),
                }],
            },
        ],
    }
}
